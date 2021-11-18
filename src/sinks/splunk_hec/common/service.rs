use std::{
    sync::Arc,
    task::{Context, Poll},
};

use crate::sinks::{
    splunk_hec::common::{build_request, request::HecRequest, response::HecResponse},
    util::http::HttpBatchService,
};
use futures_util::future::BoxFuture;
use http::Request;
use serde::Deserialize;
use tokio::sync::{mpsc::{self, UnboundedSender}, oneshot::{self, Sender}};
use tower::{Service, ServiceExt};
use vector_core::event::EventStatus;

use crate::{http::HttpClient, sinks::util::Compression};

use super::acknowledgements::run_acknowledgements;

#[derive(Clone)]
pub struct HecService {
    pub batch_service:
        HttpBatchService<BoxFuture<'static, Result<Request<Vec<u8>>, crate::Error>>, HecRequest>,
    ack_id_sender: UnboundedSender<(u64, Sender<bool>)>,
}

#[derive(Deserialize, Debug)]
struct HecAckResponseBody {
    text: String,
    code: u8,
    #[serde(alias = "ackId")]
    ack_id: Option<u64>,
}

impl HecService {
    pub fn new(client: HttpClient, http_request_builder: HttpRequestBuilder) -> Self {
        let event_client = client.clone();
        let ack_client = client;
        let http_request_builder = Arc::new(http_request_builder);
        // for transmitting ack_id's
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(run_acknowledgements(rx, ack_client, Arc::clone(&http_request_builder)));

        let batch_service = HttpBatchService::new(event_client, move |req| {
            let request_builder = Arc::clone(&http_request_builder);
            let future: BoxFuture<'static, Result<http::Request<Vec<u8>>, crate::Error>> =
                Box::pin(async move { request_builder.build_request(req).await });
            future
        });
        Self {
            batch_service,
            ack_id_sender: tx,
        }
    }
}

impl Service<HecRequest> for HecService {
    type Response = HecResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: HecRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();
        let ack_id_sender = self.ack_id_sender.clone();
        Box::pin(async move {
            http_service.ready().await?;
            let events_count = req.events_count;
            let events_byte_size = req.events_byte_size;
            let response = http_service.call(req).await?;
            let event_status = if response.status().is_success() {
                let body = serde_json::from_slice::<HecAckResponseBody>(response.body());
                match body {
                    Ok(body) => {
                        // If there is an ack_id, ack using it
                        if let Some(ack_id) = body.ack_id {
                            let (tx, rx) = oneshot::channel();
                            let _ = ack_id_sender.send((ack_id, tx));
                            println!("got back a status from the acknowledgements {:?}", rx.await);
                        } 
                        // Otherwise, we should return EventStatus::Delivered immediately
                    },
                    // handle body parsing errors
                    Err(_) => todo!(),
                }
                EventStatus::Delivered
            } else if response.status().is_server_error() {
                EventStatus::Errored
            } else {
                EventStatus::Failed
            };

            Ok(HecResponse {
                http_response: response,
                event_status,
                events_count,
                events_byte_size,
            })
        })
    }
}

pub struct HttpRequestBuilder {
    pub endpoint: String,
    pub token: String,
    pub compression: Compression,
}

impl HttpRequestBuilder {
    // async fn br(&self, body: Vec<u8>, path: &str) -> Result<Request<Vec<u8>>, crate::Error> {
    //     let uri = build_uri(self.endpoint.as_str(), path).context(UriParseError)?;

    //     let mut builder = Request::post(uri)
    //         .header("Content-Type", "application/json")
    //         .header("Authorization", format!("Splunk {}", self.token.as_str()))
    //         .header("X-Splunk-Request-Channel", format!("{}", SPLUNK_CHANNEL.as_str()));

    //     if let Some(ce) = self.compression.content_encoding() {
    //         builder = builder.header("Content-Encoding", ce);
    //     }

    //     builder.body(body).map_err(Into::into)
    // }

    pub async fn build_request(&self, req: HecRequest) -> Result<Request<Vec<u8>>, crate::Error> {
        build_request(
            self.endpoint.as_str(),
            self.token.as_str(),
            self.compression,
            req.body,
            "/services/collector/event",
        )
        .await
    }

    pub async fn build_ack_request(&self, body: Vec<u8>) -> Result<Request<Vec<u8>>, crate::Error> {
        build_request(
            self.endpoint.as_str(),
            self.token.as_str(),
            self.compression,
            body,
            "/services/collector/ack",
        )
        .await
    }
}
