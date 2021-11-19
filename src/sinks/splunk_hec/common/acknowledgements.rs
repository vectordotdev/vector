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

#[derive(Serialize)]
pub struct HecAckQueryRequestBody<'a> {
    acks: Vec<&'a u64>,
}

#[derive(Deserialize, Debug)]
struct HecAckQueryResponseBody {
    acks: HashMap<u64, bool>,
}

pub struct AckEventFinalizer {
    acks: HashMap<u64, (u8, Sender<EventStatus>)>,
    retry_limit: u8,
    // ack_event_status_sender: Sender<EventStatus>,
}

impl AckEventFinalizer {
    // pub fn new(retries: u8, ack_event_status_sender: Sender<EventStatus>) -> Self {
    pub fn new(retry_limit: u8) -> Self {
        Self {
            acks: HashMap::new(),
            retry_limit,
            // ack_event_status_sender,
        }
    }

    pub fn insert_ack_id(&mut self, ack_id: u64, ack_event_status_sender: Sender<EventStatus>) {
        self.acks
            .insert(ack_id, (self.retry_limit, ack_event_status_sender));
    }

    /// Remove successfully acked ack ids and notify that the event has been Delivered
    pub fn ack_success_ack_ids(&mut self, ack_ids: &[u64]) {
        for ack_id in ack_ids {
            match self.acks.remove(ack_id) {
                Some((_, ack_event_status_sender)) => {
                    let _ = ack_event_status_sender.send(EventStatus::Delivered);
                }
                None => {}
            }
        }
    }

    /// Builds an ack query body with the currently stored ack ids.
    pub fn get_ack_query_body(&mut self) -> HecAckQueryRequestBody {
        self.clear();
        HecAckQueryRequestBody {
            acks: self.acks.keys().collect::<Vec<_>>(),
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
    fn clear(&mut self) {
        self.acks.retain(|_, (retries, _)| *retries == 0);
    }
}

pub async fn run_acknowledgements(
    mut receiver: UnboundedReceiver<(u64, Sender<EventStatus>)>,
    client: HttpClient,
    http_request_builder: Arc<HttpRequestBuilder>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    // todo: pass in retry limit
    let mut ack_event_finalizer = AckEventFinalizer::new(30);
    let client = Arc::new(client);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let ack_query_body = ack_event_finalizer.get_ack_query_body();
                let ack_query_response = send_ack_query_request(client.clone(), http_request_builder.clone(), &ack_query_body).await;

                match ack_query_response {
                    Ok(ack_query_response) => {
                        let acked_ack_ids = ack_query_response.acks.iter().filter_map(|(ack_id, ack_status)| ack_status.then(|| *ack_id)).collect::<Vec<u64>>();
                        ack_event_finalizer.ack_success_ack_ids(acked_ack_ids.as_slice());
                    },
                    Err(error) => {
                        error!(message = "Unable to send ack query request", ?error);
                    },
                };
            },
            ack_info = receiver.recv() => {
                match ack_info {
                    Some((ack_id, tx)) => {
                        ack_event_finalizer.insert_ack_id(ack_id, tx);
                    },
                    None => break,
                }
            }
        }
    }
}

async fn send_ack_query_request(
    client: Arc<HttpClient>,
    http_request_builder: Arc<HttpRequestBuilder>,
    request_body: &HecAckQueryRequestBody<'_>,
) -> crate::Result<HecAckQueryResponseBody> {
    let request_body_bytes = serde_json::to_vec(request_body)?;
    let request = http_request_builder
        .build_ack_request(request_body_bytes)
        .await?;

    let response = client.send(request.map(Body::from)).await?;
    let response_body = hyper::body::to_bytes(response.into_body()).await?;
    serde_json::from_slice::<HecAckQueryResponseBody>(&response_body).map_err(Into::into)
}
