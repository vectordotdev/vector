use std::{
    fmt,
    sync::Arc,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_util::{future::BoxFuture, ready};
use http::Request;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use tokio::sync::{
    mpsc::{self},
    oneshot, OwnedSemaphorePermit, Semaphore,
};
use tokio_util::sync::PollSemaphore;
use tower::Service;
use uuid::Uuid;
use vector_core::event::EventStatus;

use super::acknowledgements::{run_acknowledgements, HecClientAcknowledgementsConfig};
use crate::{
    http::HttpClient,
    internal_events::{SplunkIndexerAcknowledgementUnavailableError, SplunkResponseParseError},
    sinks::{
        splunk_hec::common::{build_uri, request::HecRequest, response::HecResponse},
        util::{sink::Response, Compression},
        UriParseError,
    },
};

pub struct HecService<S> {
    pub inner: S,
    ack_finalizer_tx: Option<mpsc::Sender<(u64, oneshot::Sender<EventStatus>)>>,
    ack_slots: PollSemaphore,
    current_ack_slot: Option<OwnedSemaphorePermit>,
}

#[derive(Deserialize, Serialize, Debug)]
struct HecAckResponseBody {
    #[serde(alias = "ackId")]
    ack_id: Option<u64>,
}

impl<S> HecService<S>
where
    S: Service<HecRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Response + ResponseExt + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    pub fn new(
        inner: S,
        ack_client: Option<HttpClient>,
        http_request_builder: Arc<HttpRequestBuilder>,
        indexer_acknowledgements: HecClientAcknowledgementsConfig,
    ) -> Self {
        let max_pending_acks = indexer_acknowledgements.max_pending_acks.get();
        let tx = if let Some(ack_client) = ack_client {
            let (tx, rx) = mpsc::channel(128);
            tokio::spawn(run_acknowledgements(
                rx,
                ack_client,
                Arc::clone(&http_request_builder),
                indexer_acknowledgements,
            ));
            Some(tx)
        } else {
            None
        };

        let ack_slots = PollSemaphore::new(Arc::new(Semaphore::new(max_pending_acks as usize)));
        Self {
            inner,
            ack_finalizer_tx: tx,
            ack_slots,
            current_ack_slot: None,
        }
    }
}

impl<S> Service<HecRequest> for HecService<S>
where
    S: Service<HecRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Response + ResponseExt + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    type Response = HecResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> std::task::Poll<Result<(), Self::Error>> {
        // Ready if indexer acknowledgements is disabled or there is room for
        // additional pending acks. Otherwise, wait until there is room.
        if self.ack_finalizer_tx.is_none() || self.current_ack_slot.is_some() {
            self.inner.poll_ready(cx).map_err(Into::into)
        } else {
            match ready!(self.ack_slots.poll_acquire(cx)) {
                Some(permit) => {
                    self.current_ack_slot.replace(permit);
                    self.inner.poll_ready(cx).map_err(Into::into)
                }
                None => Poll::Ready(Err(
                    "Indexer acknowledgements semaphore unexpectedly closed".into(),
                )),
            }
        }
    }

    fn call(&mut self, req: HecRequest) -> Self::Future {
        let ack_finalizer_tx = self.ack_finalizer_tx.clone();
        let ack_slot = self.current_ack_slot.take();

        let events_count = req.events_count;
        let events_byte_size = req.events_byte_size;
        let response = self.inner.call(req);

        Box::pin(async move {
            let response = response.await.map_err(Into::into)?;
            let event_status = if response.is_successful() {
                if let Some(ack_finalizer_tx) = ack_finalizer_tx {
                    let _ack_slot = ack_slot.expect("poll_ready not called before invoking call");
                    let body = serde_json::from_slice::<HecAckResponseBody>(response.body());
                    match body {
                        Ok(body) => {
                            if let Some(ack_id) = body.ack_id {
                                let (tx, rx) = oneshot::channel();
                                match ack_finalizer_tx.send((ack_id, tx)).await {
                                    Ok(_) => rx.await.unwrap_or(EventStatus::Rejected),
                                    // If we cannot send ack ids to the ack client, fall back to default behavior
                                    Err(_) => {
                                        emit!(&SplunkIndexerAcknowledgementUnavailableError);
                                        EventStatus::Delivered
                                    }
                                }
                            } else {
                                // Default behavior if indexer acknowledgements is disabled on the Splunk server
                                EventStatus::Delivered
                            }
                        }
                        Err(error) => {
                            // This may occur if Splunk changes the response format in future versions.
                            emit!(&SplunkResponseParseError { error });
                            EventStatus::Delivered
                        }
                    }
                } else {
                    // Default behavior if indexer acknowledgements is disabled by configuration
                    EventStatus::Delivered
                }
            } else if response.is_transient() {
                EventStatus::Errored
            } else {
                EventStatus::Rejected
            };

            Ok(HecResponse {
                event_status,
                events_count,
                events_byte_size,
            })
        })
    }
}

pub trait ResponseExt {
    fn body(&self) -> &Bytes;
}

impl ResponseExt for http::Response<Bytes> {
    fn body(&self) -> &Bytes {
        self.body()
    }
}

pub struct HttpRequestBuilder {
    pub endpoint: String,
    pub default_token: String,
    pub compression: Compression,
    // A Splunk channel must be a GUID/UUID formatted value
    // https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck#About_channels_and_sending_data
    pub channel: String,
}

impl HttpRequestBuilder {
    pub fn new(endpoint: String, default_token: String, compression: Compression) -> Self {
        let channel = Uuid::new_v4().to_hyphenated().to_string();
        Self {
            endpoint,
            default_token,
            compression,
            channel,
        }
    }

    pub fn build_request(
        &self,
        body: Vec<u8>,
        path: &str,
        passthrough_token: Option<Arc<str>>,
    ) -> Result<Request<Vec<u8>>, crate::Error> {
        let uri = build_uri(self.endpoint.as_str(), path).context(UriParseError)?;

        let mut builder = Request::post(uri)
            .header("Content-Type", "application/json")
            .header(
                "Authorization",
                format!(
                    "Splunk {}",
                    passthrough_token.unwrap_or_else(|| self.default_token.as_str().into())
                ),
            )
            .header("X-Splunk-Request-Channel", self.channel.as_str());

        if let Some(ce) = self.compression.content_encoding() {
            builder = builder.header("Content-Encoding", ce);
        }

        builder.body(body).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        num::{NonZeroU64, NonZeroU8},
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        task::Poll,
    };

    use bytes::Bytes;
    use futures_util::{future::poll_fn, poll, stream::FuturesUnordered, StreamExt};
    use tower::{util::BoxService, Service, ServiceExt};
    use vector_core::{
        config::proxy::ProxyConfig,
        event::{EventFinalizers, EventStatus},
    };
    use wiremock::{
        matchers::{header, header_exists, method, path},
        Mock, MockServer, Request, Respond, ResponseTemplate,
    };

    use crate::{
        http::HttpClient,
        sinks::{
            splunk_hec::common::{
                acknowledgements::{
                    HecAckStatusRequest, HecAckStatusResponse, HecClientAcknowledgementsConfig,
                },
                build_http_batch_service,
                request::HecRequest,
                service::{HecAckResponseBody, HecService, HttpRequestBuilder},
            },
            util::Compression,
        },
    };

    const TOKEN: &str = "token";
    static ACK_ID: AtomicU64 = AtomicU64::new(0);

    fn get_hec_service(
        endpoint: String,
        acknowledgements_config: HecClientAcknowledgementsConfig,
    ) -> HecService<BoxService<HecRequest, http::Response<Bytes>, crate::Error>> {
        let client = HttpClient::new(None, &ProxyConfig::default()).unwrap();
        let http_request_builder = Arc::new(HttpRequestBuilder::new(
            endpoint,
            String::from(TOKEN),
            Compression::default(),
        ));
        let http_service =
            build_http_batch_service(client.clone(), Arc::clone(&http_request_builder));
        HecService::new(
            BoxService::new(http_service),
            Some(client),
            http_request_builder,
            acknowledgements_config,
        )
    }

    fn get_hec_request() -> HecRequest {
        let body = String::from("test-message").into_bytes();
        let events_byte_size = body.len();
        HecRequest {
            body,
            events_count: 1,
            events_byte_size,
            finalizers: EventFinalizers::default(),
            passthrough_token: None,
        }
    }

    async fn get_hec_mock_server<R>(acknowledgements_enabled: bool, ack_response: R) -> MockServer
    where
        R: Respond + 'static,
    {
        // Authorization tokens and channels are required
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/services/collector/event"))
            .and(header(
                "Authorization",
                format!("Splunk {}", TOKEN).as_str(),
            ))
            .and(header_exists("X-Splunk-Request-Channel"))
            .respond_with(move |_: &Request| {
                let ack_id =
                    acknowledgements_enabled.then(|| ACK_ID.fetch_add(1, Ordering::Relaxed));
                ResponseTemplate::new(200).set_body_json(HecAckResponseBody { ack_id })
            })
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/services/collector/ack"))
            .and(header(
                "Authorization",
                format!("Splunk {}", TOKEN).as_str(),
            ))
            .and(header_exists("X-Splunk-Request-Channel"))
            .respond_with(ack_response)
            .mount(&mock_server)
            .await;

        mock_server
    }

    fn ack_response_always_succeed(req: &Request) -> ResponseTemplate {
        let req = serde_json::from_slice::<HecAckStatusRequest>(req.body.as_slice()).unwrap();
        ResponseTemplate::new(200).set_body_json(HecAckStatusResponse {
            acks: req
                .acks
                .into_iter()
                .map(|ack_id| (ack_id, true))
                .collect::<HashMap<_, _>>(),
        })
    }

    fn ack_response_always_fail(req: &Request) -> ResponseTemplate {
        let req = serde_json::from_slice::<HecAckStatusRequest>(req.body.as_slice()).unwrap();
        ResponseTemplate::new(200).set_body_json(HecAckStatusResponse {
            acks: req
                .acks
                .into_iter()
                .map(|ack_id| (ack_id, false))
                .collect::<HashMap<_, _>>(),
        })
    }

    #[tokio::test]
    async fn acknowledgements_disabled_in_config() {
        let mock_server = get_hec_mock_server(true, ack_response_always_succeed).await;

        let acknowledgements_config = HecClientAcknowledgementsConfig {
            indexer_acknowledgements_enabled: false,
            ..Default::default()
        };
        let mut service = get_hec_service(mock_server.uri(), acknowledgements_config);

        let request = get_hec_request();
        let response = service.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(EventStatus::Delivered, response.event_status)
    }

    #[tokio::test]
    async fn acknowledgements_enabled_on_server() {
        let mock_server = get_hec_mock_server(true, ack_response_always_succeed).await;

        let acknowledgements_config = HecClientAcknowledgementsConfig {
            query_interval: NonZeroU8::new(1).unwrap(),
            ..Default::default()
        };
        let mut service = get_hec_service(mock_server.uri(), acknowledgements_config);

        let mut responses = FuturesUnordered::new();
        responses.push(service.ready().await.unwrap().call(get_hec_request()));
        responses.push(service.ready().await.unwrap().call(get_hec_request()));
        responses.push(service.ready().await.unwrap().call(get_hec_request()));
        while let Some(response) = responses.next().await {
            assert_eq!(EventStatus::Delivered, response.unwrap().event_status)
        }
    }

    #[tokio::test]
    async fn acknowledgements_disabled_on_server() {
        let ack_response = |_: &Request| ResponseTemplate::new(400);
        let mock_server = get_hec_mock_server(false, ack_response).await;

        let acknowledgements_config = HecClientAcknowledgementsConfig {
            query_interval: NonZeroU8::new(1).unwrap(),
            ..Default::default()
        };
        let mut service = get_hec_service(mock_server.uri(), acknowledgements_config);

        let request = get_hec_request();
        let response = service.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(EventStatus::Delivered, response.event_status)
    }

    #[tokio::test]
    async fn acknowledgements_enabled_on_server_retry_limit_exceeded() {
        let mock_server = get_hec_mock_server(true, ack_response_always_fail).await;

        let acknowledgements_config = HecClientAcknowledgementsConfig {
            query_interval: NonZeroU8::new(1).unwrap(),
            retry_limit: NonZeroU8::new(1).unwrap(),
            ..Default::default()
        };
        let mut service = get_hec_service(mock_server.uri(), acknowledgements_config);

        let request = get_hec_request();
        let response = service.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(EventStatus::Rejected, response.event_status)
    }

    #[tokio::test]
    async fn acknowledgements_server_changed_ack_response_format() {
        let ack_response = |_: &Request| {
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!(r#"{ "new": "a new response body" }"#))
        };
        let mock_server = get_hec_mock_server(true, ack_response).await;

        let acknowledgements_config = HecClientAcknowledgementsConfig {
            query_interval: NonZeroU8::new(1).unwrap(),
            retry_limit: NonZeroU8::new(3).unwrap(),
            ..Default::default()
        };
        let mut service = get_hec_service(mock_server.uri(), acknowledgements_config);

        let request = get_hec_request();
        let response = service.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(EventStatus::Delivered, response.event_status)
    }

    #[tokio::test]
    async fn acknowledgements_enabled_on_server_ack_endpoint_failing() {
        let ack_response = |_: &Request| ResponseTemplate::new(503);
        let mock_server = get_hec_mock_server(true, ack_response).await;

        let acknowledgements_config = HecClientAcknowledgementsConfig {
            query_interval: NonZeroU8::new(1).unwrap(),
            retry_limit: NonZeroU8::new(3).unwrap(),
            ..Default::default()
        };
        let mut service = get_hec_service(mock_server.uri(), acknowledgements_config);

        let request = get_hec_request();
        let response = service.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(EventStatus::Errored, response.event_status)
    }

    #[tokio::test]
    async fn acknowledgements_server_changed_event_response_format() {
        let mock_server = get_hec_mock_server(true, ack_response_always_succeed).await;
        // Override the usual event endpoint
        Mock::given(method("POST"))
            .and(path("/services/collector/event"))
            .and(header(
                "Authorization",
                format!("Splunk {}", TOKEN).as_str(),
            ))
            .and(header_exists("X-Splunk-Request-Channel"))
            .respond_with(move |_: &Request| {
                ResponseTemplate::new(200).set_body_json(r#"{ "new": "a new response body" }"#)
            })
            .mount(&mock_server)
            .await;

        let acknowledgements_config = HecClientAcknowledgementsConfig {
            query_interval: NonZeroU8::new(1).unwrap(),
            retry_limit: NonZeroU8::new(1).unwrap(),
            ..Default::default()
        };
        let mut service = get_hec_service(mock_server.uri(), acknowledgements_config);

        let request = get_hec_request();
        let response = service.ready().await.unwrap().call(request).await.unwrap();
        assert_eq!(EventStatus::Delivered, response.event_status)
    }

    #[tokio::test]
    async fn service_poll_ready_multiple_times() {
        let mock_server = get_hec_mock_server(true, ack_response_always_fail).await;
        let mut service = get_hec_service(mock_server.uri(), Default::default());

        assert!(service.ready().await.is_ok());
        // Consecutive poll_ready returns OK since an ack slot has been granted
        // but has not been used (call has not been invoked)
        assert!(service.ready().await.is_ok());
    }

    #[tokio::test]
    #[should_panic]
    async fn service_call_without_poll_ready() {
        let mock_server = get_hec_mock_server(true, ack_response_always_fail).await;
        let mut service = get_hec_service(mock_server.uri(), Default::default());

        let _ = service.call(get_hec_request()).await;
    }

    #[tokio::test]
    async fn acknowledgements_max_pending_acks_reached() {
        let mock_server = get_hec_mock_server(true, ack_response_always_fail).await;

        let acknowledgements_config = HecClientAcknowledgementsConfig {
            query_interval: NonZeroU8::new(1).unwrap(),
            retry_limit: NonZeroU8::new(5).unwrap(),
            // Allow a single pending ack
            max_pending_acks: NonZeroU64::new(1).unwrap(),
            ..Default::default()
        };
        let mut service = get_hec_service(mock_server.uri(), acknowledgements_config);

        // Grab the one available ack slot
        let pending_call = service.ready().await.unwrap().call(get_hec_request());
        // The service should return pending for additional requests
        assert!(matches!(
            poll!(poll_fn(|cx| service.poll_ready(cx))),
            Poll::Pending
        ));
        // Complete the call to free up the slot
        let response = pending_call.await.unwrap();
        assert_eq!(EventStatus::Rejected, response.event_status);
        // The service should now be ready for additional requests
        assert!(matches!(
            poll!(poll_fn(|cx| service.poll_ready(cx))),
            Poll::Ready(Ok(_))
        ));
    }
}
