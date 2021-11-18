use std::{collections::HashMap, sync::Arc, time::Duration};

use hyper::Body;
use serde::Serialize;
use tokio::{sync::{mpsc::UnboundedReceiver, oneshot::Sender}};

use crate::http::HttpClient;

use super::service::HttpRequestBuilder;

#[derive(Serialize)]
struct HecAckQueryRequestBody<'a> {
    acks: Vec<&'a u64>,
}

pub async fn run_acknowledgements(mut receiver: UnboundedReceiver<(u64, Sender<bool>)>, client: HttpClient, http_request_builder: Arc<HttpRequestBuilder>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    let mut ack_map: HashMap<u64, Sender<bool>> = HashMap::new();

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // make http request
                println!("...making http request to ack...");
                let body = HecAckQueryRequestBody {
                    acks: ack_map.keys().collect::<Vec<_>>(),
                };
                let req = http_request_builder.build_ack_request(serde_json::to_vec(&body).unwrap()).await.unwrap();
                client.send(req.map(Body::from));
                // pass through splunk endpoint .post("http://0.0.0.0:8080/services/collector/ack") .body() .send() .await .unwrap() .status()
            },
            ack_info = receiver.recv() => {
                match ack_info {
                    Some((ack_id, tx)) => {
                        println!("received ack id {:?}", ack_id);
                        ack_map.insert(ack_id, tx);
                    },
                    None => break,
                }       
            }
        }
    }
}