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
    acks: HashMap<u64, (u8, Sender<EventStatus>)>,
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

    /// Adds an ack id to be queried
    fn add(&mut self, ack_id: u64, ack_event_status_sender: Sender<EventStatus>) {
        self.acks
            .insert(ack_id, (self.retry_limit, ack_event_status_sender));
        emit!(SplunkIndexerAcknowledgementAckAdded);
    }

    /// Queries Splunk HEC with stored ack ids and finalizes events that are successfully acked
    async fn run(&mut self) {
        let ack_query_body = self.get_ack_query_body();
        if !ack_query_body.acks.is_empty() {
            let ack_query_response = self.send_ack_query_request(&ack_query_body).await;

            match ack_query_response {
                Ok(ack_query_response) => {
                    debug!(message = "Received ack statuses.", ?ack_query_response);
                    let acked_ack_ids = ack_query_response
                        .acks
                        .iter()
                        .filter(|&(_ack_id, ack_status)| *ack_status)
                        .map(|(ack_id, _ack_status)| *ack_id)
                        .collect::<Vec<u64>>();
                    self.finalize_delivered_ack_ids(acked_ack_ids.as_slice());
                    self.expire_ack_ids_with_status(EventStatus::Rejected);
                }
                Err(error) => {
                    match error {
                        HecAckApiError::ClientParseResponse | HecAckApiError::ClientSendQuery => {
                            // If we are permanently unable to interact with
                            // Splunk HEC indexer acknowledgements (e.g. due to
                            // request/response format changes in future
                            // versions), log an error and fall back to default
                            // behavior.
                            emit!(SplunkIndexerAcknowledgementAPIError {
                                message: "Unable to use indexer acknowledgements. Acknowledging based on initial 200 OK.",
                                error,
                            });
                            self.finalize_delivered_ack_ids(
                                self.acks.keys().copied().collect::<Vec<_>>().as_slice(),
                            );
                        }
                        _ => {
                            emit!(SplunkIndexerAcknowledgementAPIError {
                                message:
                                    "Unable to send acknowledgement query request. Will retry.",
                                error,
                            });
                            self.expire_ack_ids_with_status(EventStatus::Errored);
                        }
                    }
                }
            };
        }
    }

    /// Removes successfully acked ack ids and finalizes associated events
    fn finalize_delivered_ack_ids(&mut self, ack_ids: &[u64]) {
        let mut removed_count = 0.0;
        for ack_id in ack_ids {
            if let Some((_, ack_event_status_sender)) = self.acks.remove(ack_id) {
                _ = ack_event_status_sender.send(EventStatus::Delivered);
                removed_count += 1.0;
                debug!(message = "Finalized ack id.", ?ack_id);
            }
        }
        emit!(SplunkIndexerAcknowledgementAcksRemoved {
            count: removed_count
        });
    }

    /// Builds an ack query body with stored ack ids
    fn get_ack_query_body(&mut self) -> HecAckStatusRequest {
        HecAckStatusRequest {
            acks: self.acks.keys().copied().collect::<Vec<u64>>(),
        }
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
        let expired_ack_ids = self
            .acks
            .iter()
            .filter_map(|(ack_id, (retries, _))| (*retries == 0).then_some(*ack_id))
            .collect::<Vec<_>>();
        let mut removed_count = 0.0;
        for ack_id in expired_ack_ids {
            if let Some((_, ack_event_status_sender)) = self.acks.remove(&ack_id) {
                _ = ack_event_status_sender.send(status);
                removed_count += 1.0;
            }
        }
        emit!(SplunkIndexerAcknowledgementAcksRemoved {
            count: removed_count
        });
    }

    // Sends an ack status query request to Splunk HEC
    async fn send_ack_query_request(
        &mut self,
        request_body: &HecAckStatusRequest,
    ) -> Result<HecAckStatusResponse, HecAckApiError> {
        self.decrement_retries();
        let request_body_bytes = crate::serde::json::to_bytes(request_body)
            .map_err(|_| HecAckApiError::ClientBuildRequest)?
            .freeze();
        let request = self
            .http_request_builder
            .build_request(
                request_body_bytes,
                "/services/collector/ack",
                None,
                MetadataFields::default(),
                false,
            )
            .map_err(|_| HecAckApiError::ClientBuildRequest)?;

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
    mut receiver: Receiver<(u64, Sender<EventStatus>)>,
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
                    Some((ack_id, tx)) => {
                        ack_client.add(ack_id, tx);
                        debug!(message = "Stored ack id.", ?ack_id);
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
            splunk_hec::common::{
                acknowledgements::HecAckStatusRequest, service::HttpRequestBuilder, EndpointTarget,
            },
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
            ack_client.add(*ack_id, tx);
            ack_status_rxs.push(rx);
        }
        ack_status_rxs
    }

    #[test]
    fn test_get_ack_query_body() {
        let mut ack_client = get_ack_client(1);
        let ack_ids = (0..100).collect::<Vec<u64>>();
        _ = populate_ack_client(&mut ack_client, &ack_ids);
        let expected_ack_body = HecAckStatusRequest { acks: ack_ids };

        let mut ack_request_body = ack_client.get_ack_query_body();
        ack_request_body.acks.sort_unstable();
        assert_eq!(expected_ack_body, ack_request_body);
    }

    #[test]
    fn test_decrement_retries() {
        let mut ack_client = get_ack_client(1);
        let ack_ids = (0..100).collect::<Vec<u64>>();
        _ = populate_ack_client(&mut ack_client, &ack_ids);

        let mut ack_request_body = ack_client.get_ack_query_body();
        ack_request_body.acks.sort_unstable();
        assert_eq!(ack_ids, ack_request_body.acks);
        ack_client.decrement_retries();
        ack_client.expire_ack_ids_with_status(EventStatus::Rejected);

        let ack_request_body = ack_client.get_ack_query_body();
        assert!(ack_request_body.acks.is_empty())
    }

    #[tokio::test]
    async fn test_finalize_delivered_ack_ids() {
        let mut ack_client = get_ack_client(1);
        let ack_ids = (0..100).collect::<Vec<u64>>();
        let ack_status_rxs = populate_ack_client(&mut ack_client, &ack_ids);

        ack_client.finalize_delivered_ack_ids(ack_ids.as_slice());
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
}
