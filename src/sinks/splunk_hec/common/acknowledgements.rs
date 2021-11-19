use std::{collections::HashMap, sync::Arc, time::Duration};

use hyper::Body;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::UnboundedReceiver, oneshot::Sender};
use vector_core::event::EventStatus;

use crate::http::HttpClient;

use super::service::HttpRequestBuilder;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HecClientAcknowledgementsConfig {
    query_interval: u8,
    retry_limit: u8,
}

impl Default for HecClientAcknowledgementsConfig {
    fn default() -> Self {
        Self {
            query_interval: 10,
            retry_limit: 30,
        }
    }
}

#[derive(Serialize, Eq, PartialEq, Debug)]
pub struct HecAckQueryRequestBody {
    pub acks: Vec<u64>,
}

#[derive(Deserialize, Debug)]
struct HecAckQueryResponseBody {
    acks: HashMap<u64, bool>,
}

pub struct AckEventFinalizer {
    acks: HashMap<u64, (u8, Sender<EventStatus>)>,
    retry_limit: u8,
}

impl AckEventFinalizer {
    pub fn new(retry_limit: u8) -> Self {
        Self {
            acks: HashMap::new(),
            retry_limit,
        }
    }

    pub fn insert(&mut self, ack_id: u64, ack_event_status_sender: Sender<EventStatus>) {
        self.acks
            .insert(ack_id, (self.retry_limit, ack_event_status_sender));
    }

    /// Remove successfully acked ack ids and notify that the event has been Delivered
    pub fn finalize_ack_ids(&mut self, ack_ids: &[u64]) {
        for ack_id in ack_ids {
            match self.acks.remove(ack_id) {
                Some((_, ack_event_status_sender)) => {
                    let _ = ack_event_status_sender.send(EventStatus::Delivered);
                    debug!("Finalized ack id {:?}", ack_id);
                }
                None => {}
            }
        }
    }

    /// Builds an ack query body with the currently stored ack ids.
    pub fn get_ack_query_body(&mut self) -> HecAckQueryRequestBody {
        self.clear_expired_ack_ids();
        HecAckQueryRequestBody {
            acks: self.acks.keys().map(|id| *id).collect::<Vec<u64>>(),
        }
    }

    /// Decrements retry count on all ack ids by 1
    pub fn decrement_retries(&mut self) {
        for (retries, _) in self.acks.values_mut() {
            if *retries > 0 {
                *retries -= 1;
            }
        }
    }

    /// Removes all expired ack ids (those with a retry count of 0).
    fn clear_expired_ack_ids(&mut self) {
        self.acks.retain(|_, (retries, _)| *retries > 0);
    }
}

pub async fn run_acknowledgements(
    mut receiver: UnboundedReceiver<(u64, Sender<EventStatus>)>,
    client: HttpClient,
    http_request_builder: Arc<HttpRequestBuilder>,
    indexer_acknowledgements: HecClientAcknowledgementsConfig,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(
        indexer_acknowledgements.query_interval as u64,
    ));
    let mut ack_event_finalizer = AckEventFinalizer::new(indexer_acknowledgements.retry_limit);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let ack_query_body = ack_event_finalizer.get_ack_query_body();
                let ack_query_response = send_ack_query_request(&client, http_request_builder.clone(), &ack_query_body).await;

                match ack_query_response {
                    Ok(ack_query_response) => {
                        debug!("Received ack statuses {:?}", ack_query_response);
                        ack_event_finalizer.decrement_retries();
                        let acked_ack_ids = ack_query_response.acks.iter().filter_map(|(ack_id, ack_status)| ack_status.then(|| *ack_id)).collect::<Vec<u64>>();
                        ack_event_finalizer.finalize_ack_ids(acked_ack_ids.as_slice());
                    },
                    Err(error) => {
                        error!(message = "Unable to send ack query request", ?error);
                    },
                };
            },
            ack_info = receiver.recv() => {
                match ack_info {
                    Some((ack_id, tx)) => {
                        ack_event_finalizer.insert(ack_id, tx);
                        debug!("Stored ack id {}", ack_id);
                    },
                    None => break,
                }
            }
        }
    }
}

async fn send_ack_query_request(
    client: &HttpClient,
    http_request_builder: Arc<HttpRequestBuilder>,
    request_body: &HecAckQueryRequestBody,
) -> crate::Result<HecAckQueryResponseBody> {
    let request_body_bytes = serde_json::to_vec(request_body)?;
    let request = http_request_builder
        .build_ack_request(request_body_bytes)
        .await?;

    let response = client.send(request.map(Body::from)).await?;
    let response_body = hyper::body::to_bytes(response.into_body()).await?;
    serde_json::from_slice::<HecAckQueryResponseBody>(&response_body).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use futures_util::{StreamExt, stream::FuturesUnordered};
    use tokio::sync::oneshot::{self, Receiver};
    use vector_core::event::EventStatus;

    use crate::sinks::splunk_hec::common::acknowledgements::HecAckQueryRequestBody;

    use super::AckEventFinalizer;

    fn populate_ack_finalizer(ack_finalizer: &mut AckEventFinalizer, ack_ids: &Vec<u64>) -> Vec<Receiver<EventStatus>> {
        let mut ack_status_rxs = Vec::new();
        for ack_id in ack_ids {
            let (tx, rx) = oneshot::channel();
            ack_finalizer.insert(*ack_id, tx);
            ack_status_rxs.push(rx);
        }
        ack_status_rxs
    }

    #[test]
    fn test_get_ack_query_body() {
        let mut ack_finalizer = AckEventFinalizer::new(1);
        let ack_ids = (0..100).collect::<Vec<u64>>(); 
        let _ = populate_ack_finalizer(&mut ack_finalizer, &ack_ids);
        let expected_ack_body = HecAckQueryRequestBody {
            acks: ack_ids,
        };

        let mut ack_request_body = ack_finalizer.get_ack_query_body();
        ack_request_body.acks.sort();
        assert_eq!(expected_ack_body, ack_request_body);
    }

    #[test]
    fn test_decrement_retries() {
        let mut ack_finalizer = AckEventFinalizer::new(1);
        let ack_ids = (0..100).collect::<Vec<u64>>(); 
        let _ = populate_ack_finalizer(&mut ack_finalizer, &ack_ids);

        let mut ack_request_body = ack_finalizer.get_ack_query_body();
        ack_request_body.acks.sort();
        assert_eq!(ack_ids, ack_request_body.acks);
        ack_finalizer.decrement_retries();

        let ack_request_body= ack_finalizer.get_ack_query_body();
        assert!(ack_request_body.acks.is_empty())
    }

    #[tokio::test]
    async fn test_finalize_ack_ids() {
        let mut ack_finalizer = AckEventFinalizer::new(1);
        let ack_ids = (0..100).collect::<Vec<u64>>(); 
        let ack_status_rxs = populate_ack_finalizer(&mut ack_finalizer, &ack_ids);

        ack_finalizer.finalize_ack_ids(ack_ids.as_slice());
        let mut statuses = ack_status_rxs.into_iter().collect::<FuturesUnordered<_>>();
        while let Some(status) = statuses.next().await {
            assert_eq!(EventStatus::Delivered, status.unwrap());
        }
    }
}