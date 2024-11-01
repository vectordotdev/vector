use std::{
    fmt,
    sync::Arc,
    task::{ready, Context, Poll},
};

use bytes::Bytes;
use futures_util::future::BoxFuture;
use http::Request;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::PollSemaphore;
use tower::Service;
use uuid::Uuid;
use vector_lib::event::EventStatus;
use vector_lib::request_metadata::MetaDescriptive;

use super::{
    acknowledgements::{run_acknowledgements, HecClientAcknowledgementsConfig},
    EndpointTarget,
};
use crate::{
    http::HttpClient,
    internal_events::{SplunkIndexerAcknowledgementUnavailableError, SplunkResponseParseError},
    sinks::{
        splunk_hec::common::{build_uri, request::HecRequest, response::HecResponse},
        util::{sink::Response, Compression},
        UriParseSnafu,
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

    fn call(&mut self, mut req: HecRequest) -> Self::Future {
        let ack_finalizer_tx = self.ack_finalizer_tx.clone();
        let ack_slot = self.current_ack_slot.take();

        let metadata = std::mem::take(req.metadata_mut());
        let events_count = metadata.event_count();
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();
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
                                    Err(error) => {
                                        emit!(SplunkIndexerAcknowledgementUnavailableError {
                                            error
                                        });
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
                            emit!(SplunkResponseParseError { error });
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
    pub endpoint_target: EndpointTarget,
    pub endpoint: String,
    pub default_token: String,
    pub compression: Compression,
    // A Splunk channel must be a GUID/UUID formatted value
    // https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck#About_channels_and_sending_data
    pub channel: String,
}

#[derive(Default)]
pub(super) struct MetadataFields {
    pub(super) source: Option<String>,
    pub(super) sourcetype: Option<String>,
    pub(super) index: Option<String>,
    pub(super) host: Option<String>,
}

impl HttpRequestBuilder {
    pub fn new(
        endpoint: String,
        endpoint_target: EndpointTarget,
        default_token: String,
        compression: Compression,
    ) -> Self {
        let channel = Uuid::new_v4().hyphenated().to_string();
        Self {
            endpoint,
            endpoint_target,
            default_token,
            compression,
            channel,
        }
    }

    pub(super) fn build_request(
        &self,
        body: Bytes,
        path: &str,
        passthrough_token: Option<Arc<str>>,
        metadata_fields: MetadataFields,
        auto_extract_timestamp: bool,
    ) -> Result<Request<Bytes>, crate::Error> {
        let uri = match self.endpoint_target {
            EndpointTarget::Raw => {
                // `auto_extract_timestamp` doesn't apply to the raw endpoint since the raw endpoint
                // always does this anyway.
                let metadata = [
                    (super::SOURCE_FIELD, metadata_fields.source),
                    (super::SOURCETYPE_FIELD, metadata_fields.sourcetype),
                    (super::INDEX_FIELD, metadata_fields.index),
                    (super::HOST_FIELD, metadata_fields.host),
                ]
                .into_iter()
                .filter_map(|(key, value)| value.map(|value| (key, value)));
                build_uri(self.endpoint.as_str(), path, metadata).context(UriParseSnafu)?
            }
            EndpointTarget::Event => build_uri(
                self.endpoint.as_str(),
                path,
                if auto_extract_timestamp {
                    Some((super::AUTO_EXTRACT_TIMESTAMP_FIELD, "true".to_string()))
                } else {
                    None
                },
            )
            .context(UriParseSnafu)?,
        };

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
        future::poll_fn,
        num::{NonZeroU64, NonZeroU8, NonZeroUsize},
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        task::Poll,
    };

    use bytes::Bytes;
    use futures_util::{poll, stream::FuturesUnordered, StreamExt};
    use tower::{util::BoxService, Service, ServiceExt};
    use vector_lib::internal_event::CountByteSize;
    use vector_lib::{
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
                EndpointTarget,
            },
            util::{metadata::RequestMetadataBuilder, Compression},
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
            EndpointTarget::default(),
            String::from(TOKEN),
            Compression::default(),
        ));
        let http_service = build_http_batch_service(
            client.clone(),
            Arc::clone(&http_request_builder),
            EndpointTarget::Event,
            false,
        );
        HecService::new(
            BoxService::new(http_service),
            Some(client),
            http_request_builder,
            acknowledgements_config,
        )
    }

    fn get_hec_request() -> HecRequest {
        let body = Bytes::from("test-message");
        let events_byte_size = body.len();

        let builder = RequestMetadataBuilder::new(
            1,
            events_byte_size,
            CountByteSize(1, events_byte_size.into()).into(),
        );
        let bytes_len =
            NonZeroUsize::new(events_byte_size).expect("payload should never be zero length");
        let metadata = builder.with_request_size(bytes_len);

        HecRequest {
            body,
            metadata,
            finalizers: EventFinalizers::default(),
            passthrough_token: None,
            index: None,
            source: None,
            sourcetype: None,
            host: None,
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

        _ = service.call(get_hec_request()).await;
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
