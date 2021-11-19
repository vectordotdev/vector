use std::{collections::HashMap, sync::Arc, time::Duration};

use hyper::Body;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::UnboundedReceiver, oneshot::Sender};

use crate::http::HttpClient;

use super::service::HttpRequestBuilder;

#[derive(Deserialize, Clone, Debug)]
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
struct HecAckQueryRequestBody<'a> {
    acks: Vec<&'a u64>,
}

#[derive(Deserialize, Debug)]
struct HecAckQueryResponseBody {
    acks: HashMap<u64, bool>,
}

pub async fn run_acknowledgements(
    mut receiver: UnboundedReceiver<(u64, Sender<bool>)>,
    client: HttpClient,
    http_request_builder: Arc<HttpRequestBuilder>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    let mut ack_map: HashMap<u64, Sender<bool>> = HashMap::new();

    loop {
        tokio::select! {
            _ = interval.tick() => {
                println!("...making http request to ack...");
                let body = HecAckQueryRequestBody {
                    acks: ack_map.keys().collect::<Vec<_>>(),
                };
                // todo: refactor the build_ack_request: does it need to be async?
                let req = http_request_builder.build_ack_request(serde_json::to_vec(&body).unwrap()).await.unwrap();
                // todo: handle error
                let res = client.send(req.map(Body::from)).await.unwrap();

                let body = hyper::body::to_bytes(res.into_body()).await.unwrap();
                let ack_body = serde_json::from_slice::<HecAckQueryResponseBody>(&body).unwrap();
                let acked = ack_body.acks.iter().filter_map(|(ack_id, ack_status)| {
                    if *ack_status {
                        Some(*ack_id)
                    } else {
                        None
                    }
                }).collect::<Vec<_>>();
                for ack_id in acked {
                    match ack_map.remove(&ack_id) {
                        Some(tx) => tx.send(true).unwrap(),
                        None => {
                            // this should be unreachable since the request uses
                        },
                    }
                }
            },
            ack_info = receiver.recv() => {
                match ack_info {
                    Some((ack_id, tx)) => {
                        ack_map.insert(ack_id, tx);
                    },
                    None => break,
                }
            }
        }
    }
}
