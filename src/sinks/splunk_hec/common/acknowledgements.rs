use std::{
    collections::HashMap,
    num::{NonZeroU64, NonZeroU8},
    sync::Arc,
    time::Duration,
};

use hyper::Body;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::Receiver, oneshot::Sender};
use vector_lib::configurable::configurable_component;
use vector_lib::event::EventStatus;

use super::service::{HttpRequestBuilder, MetadataFields};
use crate::{
    config::AcknowledgementsConfig,
    http::HttpClient,
    internal_events::{
        SplunkIndexerAcknowledgementAPIError, SplunkIndexerAcknowledgementAckAdded,
        SplunkIndexerAcknowledgementAcksRemoved,
    },
};

/// Splunk HEC acknowledgement configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(default)]
#[configurable(metadata(docs::advanced))]
pub struct HecClientAcknowledgementsConfig {
    /// Controls if the sink integrates with [Splunk HEC indexer acknowledgements][splunk_indexer_ack_docs] for end-to-end acknowledgements.
    ///
    /// [splunk_indexer_ack_docs]: https://docs.splunk.com/Documentation/Splunk/8.2.3/Data/AboutHECIDXAck
    pub indexer_acknowledgements_enabled: bool,

    /// The amount of time to wait between queries to the Splunk HEC indexer acknowledgement endpoint.
    #[configurable(metadata(docs::type_unit = "seconds"))]
    pub query_interval: NonZeroU8,

    /// The maximum number of times an acknowledgement ID is queried for its status.
    pub retry_limit: NonZeroU8,

    /// The maximum number of pending acknowledgements from events sent to the Splunk HEC collector.
    ///
    /// Once reached, the sink begins applying backpressure.
    pub max_pending_acks: NonZeroU64,

    /// Specifies the name of a cookie to extract from the Splunk HEC response and use when querying for acknowledgements.
    ///
    /// This is useful when using a load balancer in front of multiple Splunk indexers in a cluster because the
    /// request to check for acknowledgements needs to go to the same indexer that originally received the data,
    /// and the cookie can help with that routing.
    ///
    /// If empty, no cookie will be extracted.
    pub cookie_name: String,

    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        flatten,
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub inner: AcknowledgementsConfig,
}

impl Default for HecClientAcknowledgementsConfig {
    fn default() -> Self {
        Self {
            indexer_acknowledgements_enabled: true,
            query_interval: NonZeroU8::new(10).unwrap(),
            retry_limit: NonZeroU8::new(30).unwrap(),
            max_pending_acks: NonZeroU64::new(1_000_000).unwrap(),
            cookie_name: String::new(),
            inner: Default::default(),
        }
    }
}

#[derive(Deserialize, Serialize, Eq, PartialEq, Debug)]
pub struct HecAckStatusRequest {
    pub acks: Vec<u64>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct HecAckStatusResponse {
    pub acks: HashMap<u64, bool>,
}

#[derive(Debug)]
pub enum HecAckApiError {
    ClientBuildRequest,
    ClientParseResponse,
    ClientSendQuery,
    ServerSendQuery,
}

struct HecAckClient {
    // Maps (ack_id, cookie_string) to (retry_count, status_sender)
    acks: HashMap<(u64, String), (u8, Sender<EventStatus>)>,
    retry_limit: u8,
    client: HttpClient,
    http_request_builder: Arc<HttpRequestBuilder>,
}

impl HecAckClient {
    fn new(
        retry_limit: u8,
        client: HttpClient,
        http_request_builder: Arc<HttpRequestBuilder>,
    ) -> Self {
        Self {
            acks: HashMap::new(),
            retry_limit,
            client,
            http_request_builder,
        }
    }

    /// Adds an ack id to be queried with a cookie string
    fn add(&mut self, ack_id: u64, cookie: String, ack_event_status_sender: Sender<EventStatus>) {
        self.acks.insert(
            (ack_id, cookie),
            (self.retry_limit, ack_event_status_sender),
        );
        emit!(SplunkIndexerAcknowledgementAckAdded);
    }

    /// Queries Splunk HEC with stored ack ids and finalizes events that are successfully acked
    async fn run(&mut self) {
        // Group ack IDs by cookie string (cookie may be an empty string for single-indexer clusters)
        let ack_groups = self.get_ack_groups_by_cookie();
        let mut error_sending_ack = false;

        // Decrement retries once every loop through all acks
        self.decrement_retries();

        for (cookie, ack_ids) in ack_groups {
            if !ack_ids.is_empty() {
                let ack_query_body = HecAckStatusRequest {
                    acks: ack_ids.clone(),
                };
                let ack_query_response =
                    self.send_ack_query_request(&ack_query_body, &cookie).await;

                match ack_query_response {
                    Ok(ack_query_response) => {
                        debug!(
                            message = "Received ack statuses for cookie.",
                            ?ack_query_response,
                            ?cookie,
                        );
                        let acked_ack_ids = ack_query_response
                            .acks
                            .iter()
                            .filter(|&(_ack_id, ack_status)| *ack_status)
                            .map(|(ack_id, _ack_status)| *ack_id)
                            .collect::<Vec<u64>>();
                        self.finalize_delivered_ack_ids(cookie, acked_ack_ids.as_slice());
                    }
                    Err(error) => {
                        match error {
                            HecAckApiError::ClientParseResponse
                            | HecAckApiError::ClientSendQuery => {
                                // If we are permanently unable to interact with
                                // Splunk HEC indexer acknowledgements (e.g. due to
                                // request/response format changes in future
                                // versions), log an error and fall back to default
                                // behavior.
                                emit!(SplunkIndexerAcknowledgementAPIError {
                                    message: "Unable to use indexer acknowledgements. Acknowledging based on initial 200 OK.",
                                    error,
                                });
                                self.finalize_delivered_ack_ids(cookie, &ack_ids);
                            }
                            _ => {
                                emit!(SplunkIndexerAcknowledgementAPIError {
                                    message:
                                        "Unable to send acknowledgement query request. Will retry.",
                                    error,
                                });
                                error_sending_ack = true;
                            }
                        }
                    }
                };
            }
        }

        if error_sending_ack {
            self.expire_ack_ids_with_status(EventStatus::Errored);
        }
        self.expire_ack_ids_with_status(EventStatus::Rejected);
    }

    /// Removes successfully acked ack ids and finalizes associated events
    fn finalize_delivered_ack_ids(&mut self, cookie: String, ack_ids: &[u64]) {
        let mut removed_count = 0.0;
        for ack_id in ack_ids {
            if let Some((_, ack_event_status_sender)) = self.acks.remove(&(*ack_id, cookie.clone()))
            {
                _ = ack_event_status_sender.send(EventStatus::Delivered);
                removed_count += 1.0;
                debug!(message = "Finalized ack id.", ?ack_id, ?cookie);
            }
        }
        emit!(SplunkIndexerAcknowledgementAcksRemoved {
            count: removed_count
        });
    }

    /// Groups ack IDs by cookie string
    fn get_ack_groups_by_cookie(&self) -> HashMap<String, Vec<u64>> {
        let mut groups: HashMap<String, Vec<u64>> = HashMap::new();

        for (ack_id, cookie) in self.acks.keys() {
            groups.entry(cookie.clone()).or_default().push(*ack_id);
        }

        groups
    }

    /// Decrements retry count on all stored ack ids by 1
    fn decrement_retries(&mut self) {
        for (retries, _) in self.acks.values_mut() {
            *retries = retries.checked_sub(1).unwrap_or(0);
        }
    }

    /// Removes all expired ack ids (those with a retry count of 0) and
    /// finalizes associated events with the given status
    fn expire_ack_ids_with_status(&mut self, status: EventStatus) {
        let expired_ack_keys = self
            .acks
            .iter()
            .filter_map(|((ack_id, cookie), (retries, _))| {
                (*retries == 0).then_some((*ack_id, cookie.clone()))
            })
            .collect::<Vec<_>>();
        let mut removed_count = 0.0;
        for key in expired_ack_keys {
            debug!(message = "Expired ack with status.", ?key, ?status);
            if let Some((_, ack_event_status_sender)) = self.acks.remove(&key) {
                _ = ack_event_status_sender.send(status);
                removed_count += 1.0;
            }
        }
        emit!(SplunkIndexerAcknowledgementAcksRemoved {
            count: removed_count
        });
    }

    // Sends an ack status query request to Splunk HEC with the specified cookie
    async fn send_ack_query_request(
        &mut self,
        request_body: &HecAckStatusRequest,
        cookie: &str,
    ) -> Result<HecAckStatusResponse, HecAckApiError> {
        let request_body_bytes = crate::serde::json::to_bytes(request_body)
            .map_err(|_| HecAckApiError::ClientBuildRequest)?
            .freeze();

        let mut request = self
            .http_request_builder
            .build_request(
                request_body_bytes,
                "/services/collector/ack",
                None,
                MetadataFields::default(),
                false,
            )
            .map_err(|_| HecAckApiError::ClientBuildRequest)?;

        // Add the cookie header if it's not empty
        if !cookie.is_empty() {
            request.headers_mut().insert(
                http::header::COOKIE,
                http::header::HeaderValue::from_str(cookie)
                    .map_err(|_| HecAckApiError::ClientBuildRequest)?,
            );
        }

        let response = self
            .client
            .send(request.map(Body::from))
            .await
            .map_err(|_| HecAckApiError::ServerSendQuery)?;

        let status = response.status();
        if status.is_success() {
            let response_body = hyper::body::to_bytes(response.into_body())
                .await
                .map_err(|_| HecAckApiError::ClientParseResponse)?;
            serde_json::from_slice::<HecAckStatusResponse>(&response_body)
                .map_err(|_| HecAckApiError::ClientParseResponse)
        } else if status.is_client_error() {
            Err(HecAckApiError::ClientSendQuery)
        } else {
            Err(HecAckApiError::ServerSendQuery)
        }
    }
}

pub async fn run_acknowledgements(
    mut receiver: Receiver<(u64, String, Sender<EventStatus>)>,
    client: HttpClient,
    http_request_builder: Arc<HttpRequestBuilder>,
    indexer_acknowledgements: HecClientAcknowledgementsConfig,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(
        indexer_acknowledgements.query_interval.get() as u64,
    ));
    let mut ack_client = HecAckClient::new(
        indexer_acknowledgements.retry_limit.get(),
        client,
        http_request_builder,
    );

    loop {
        tokio::select! {
            _ = interval.tick() => {
                ack_client.run().await;
            },
            ack_info = receiver.recv() => {
                match ack_info {
                    Some((ack_id, cookie, tx)) => {
                        let cookie_str = cookie.clone();
                        ack_client.add(ack_id, cookie_str, tx);
                        debug!(message = "Stored ack id with cookie.", ?ack_id, ?cookie);
                    },
                    None => break,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures_util::{stream::FuturesUnordered, StreamExt};
    use tokio::sync::oneshot::{self, Receiver};
    use vector_lib::{config::proxy::ProxyConfig, event::EventStatus};

    use super::HecAckClient;
    use crate::{
        http::HttpClient,
        sinks::{
            splunk_hec::common::{service::HttpRequestBuilder, EndpointTarget},
            util::Compression,
        },
    };

    fn get_ack_client(retry_limit: u8) -> HecAckClient {
        let client = HttpClient::new(None, &ProxyConfig::default()).unwrap();
        let http_request_builder = HttpRequestBuilder::new(
            String::from(""),
            EndpointTarget::default(),
            String::from(""),
            Compression::default(),
        );
        HecAckClient::new(retry_limit, client, Arc::new(http_request_builder))
    }

    fn populate_ack_client(
        ack_client: &mut HecAckClient,
        ack_ids: &[u64],
    ) -> Vec<Receiver<EventStatus>> {
        let mut ack_status_rxs = Vec::new();
        for ack_id in ack_ids {
            let (tx, rx) = oneshot::channel();
            ack_client.add(*ack_id, String::new(), tx);
            ack_status_rxs.push(rx);
        }
        ack_status_rxs
    }

    #[test]
    fn test_get_ack_groups_by_cookie_multiple_groups() {
        let mut ack_client = get_ack_client(1);

        let (tx1, _) = oneshot::channel();
        ack_client.add(1, "cookie1".to_string(), tx1);

        let (tx2, _) = oneshot::channel();
        ack_client.add(2, "cookie1".to_string(), tx2);

        let (tx3, _) = oneshot::channel();
        ack_client.add(3, "cookie2".to_string(), tx3);

        let groups = ack_client.get_ack_groups_by_cookie();

        assert_eq!(groups.len(), 2);
        assert!(groups.contains_key("cookie1"));
        assert!(groups.contains_key("cookie2"));

        let mut cookie1_acks = groups.get("cookie1").unwrap().clone();
        cookie1_acks.sort_unstable();
        assert_eq!(cookie1_acks, vec![1, 2]);

        assert_eq!(groups.get("cookie2").unwrap(), &vec![3]);
    }

    #[test]
    fn test_get_ack_groups_by_cookie_single_group() {
        let mut ack_client = get_ack_client(1);
        let expected_ack_ids = (0..100).collect::<Vec<u64>>();
        _ = populate_ack_client(&mut ack_client, &expected_ack_ids);

        let groups = ack_client.get_ack_groups_by_cookie();
        assert_eq!(groups.len(), 1);
        let mut ack_ids = Vec::new();
        for (_, ids) in groups {
            ack_ids.extend(ids);
        }
        ack_ids.sort_unstable();

        assert_eq!(expected_ack_ids, ack_ids);
    }

    #[test]
    fn test_decrement_retries() {
        let mut ack_client = get_ack_client(1);
        let ack_ids = (0..100).collect::<Vec<u64>>();
        _ = populate_ack_client(&mut ack_client, &ack_ids);

        let groups = ack_client.get_ack_groups_by_cookie();
        let mut initial_ack_ids = Vec::new();
        for (_, ids) in groups {
            initial_ack_ids.extend(ids);
        }
        initial_ack_ids.sort_unstable();
        assert_eq!(ack_ids, initial_ack_ids);

        ack_client.decrement_retries();
        ack_client.expire_ack_ids_with_status(EventStatus::Rejected);

        let groups = ack_client.get_ack_groups_by_cookie();
        let mut final_ack_ids = Vec::new();
        for (_, ids) in groups {
            final_ack_ids.extend(ids);
        }
        assert!(final_ack_ids.is_empty())
    }

    #[tokio::test]
    async fn test_finalize_delivered_ack_ids() {
        let mut ack_client = get_ack_client(1);
        let ack_ids = (0..100).collect::<Vec<u64>>();
        let ack_status_rxs = populate_ack_client(&mut ack_client, &ack_ids);

        ack_client.finalize_delivered_ack_ids(String::new(), ack_ids.as_slice());
        let mut statuses = ack_status_rxs.into_iter().collect::<FuturesUnordered<_>>();
        while let Some(status) = statuses.next().await {
            assert_eq!(EventStatus::Delivered, status.unwrap());
        }
    }

    #[tokio::test]
    async fn test_expire_ack_ids_with_status() {
        let mut ack_client = get_ack_client(1);
        let ack_ids = (0..100).collect::<Vec<u64>>();
        let ack_status_rxs = populate_ack_client(&mut ack_client, &ack_ids);

        ack_client.decrement_retries();
        ack_client.expire_ack_ids_with_status(EventStatus::Rejected);
        let mut statuses = ack_status_rxs.into_iter().collect::<FuturesUnordered<_>>();
        while let Some(status) = statuses.next().await {
            assert_eq!(EventStatus::Rejected, status.unwrap());
        }
    }

    #[tokio::test]
    async fn test_run_with_cookies() {
        use super::{HecAckStatusRequest, HecAckStatusResponse};
        use std::collections::HashMap;
        use wiremock::{
            matchers::{header, method, path},
            Mock, MockServer, Request, ResponseTemplate,
        };

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/services/collector/ack"))
            .and(header("Cookie", "cookie1"))
            .respond_with(|req: &Request| {
                let req_body =
                    serde_json::from_slice::<HecAckStatusRequest>(req.body.as_slice()).unwrap();
                ResponseTemplate::new(200).set_body_json(HecAckStatusResponse {
                    acks: req_body
                        .acks
                        .into_iter()
                        .map(|ack_id| (ack_id, true))
                        .collect::<HashMap<_, _>>(),
                })
            })
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/services/collector/ack"))
            .and(header("Cookie", "cookie2"))
            .respond_with(|req: &Request| {
                let req_body =
                    serde_json::from_slice::<HecAckStatusRequest>(req.body.as_slice()).unwrap();

                // Set this to false for cookie2 to simulate the indexer not processing this ack yet
                ResponseTemplate::new(200).set_body_json(HecAckStatusResponse {
                    acks: req_body
                        .acks
                        .into_iter()
                        .map(|ack_id| (ack_id, false))
                        .collect::<HashMap<_, _>>(),
                })
            })
            .mount(&mock_server)
            .await;

        let client = HttpClient::new(None, &ProxyConfig::default()).unwrap();
        let http_request_builder = HttpRequestBuilder::new(
            mock_server.uri(),
            EndpointTarget::default(),
            String::from("token"),
            Compression::default(),
        );
        let mut ack_client = HecAckClient::new(1, client, Arc::new(http_request_builder));

        let (tx1, rx1) = oneshot::channel();
        ack_client.add(1, "cookie1".to_string(), tx1);
        let (tx2, rx2) = oneshot::channel();
        ack_client.add(2, "cookie1".to_string(), tx2);
        let (tx3, rx3) = oneshot::channel();
        ack_client.add(3, "cookie2".to_string(), tx3);

        ack_client.run().await;

        // Verify that all acks with cookie1 were marked as delivered, but cookie2 was not
        let cookie1_status1 = rx1.await.unwrap();
        let cookie1_status2 = rx2.await.unwrap();
        let cookie2_status = rx3.await.unwrap();

        assert_eq!(EventStatus::Delivered, cookie1_status1);
        assert_eq!(EventStatus::Delivered, cookie1_status2);
        assert_eq!(EventStatus::Rejected, cookie2_status);
    }
}
