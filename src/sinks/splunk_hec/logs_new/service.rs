use std::{
    io,
    sync::Arc,
    task::{Context, Poll},
};

use crate::{
    internal_events::EventsSent,
    sinks::{
        util::{encoding::EncodingConfigFixed, http::HttpBatchService, ElementCount},
        UriParseError,
    },
};
use futures_util::{future::BoxFuture, FutureExt};
use http::{
    uri::{PathAndQuery, Scheme},
    Request, Uri,
};
use hyper::Body;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower::{Service, ServiceExt};
use vector_core::{
    buffers::Ackable,
    config::log_schema,
    event::{Event, EventFinalizers, EventStatus, Finalizable, LogEvent, Value},
    ByteSizeOf,
};

use crate::{
    http::HttpClient,
    internal_events::{EndpointBytesSent, SplunkEventEncodeError, SplunkEventSent},
    sinks::{
        splunk_hec::{
            common::{build_uri, render_template_string},
            logs_new,
        },
        util::{
            encoding::{Encoder, EncodingConfig, EncodingConfiguration, StandardEncodings},
            retries::RetryLogic,
            Compression, RequestBuilder,
        },
    },
    template::Template,
};
use snafu::{ResultExt, Snafu};

use super::{encoder::HecLogsEncoder, sink::ProcessedEvent};

#[derive(Clone)]
pub struct HecLogsService {
    pub batch_service: HttpBatchService<
        BoxFuture<'static, Result<Request<Vec<u8>>, crate::Error>>,
        HecLogsRequest,
    >,
}

impl HecLogsService {
    pub fn new(client: HttpClient, http_request_builder: HttpRequestBuilder) -> Self {
        let http_request_builder = Arc::new(http_request_builder);
        let batch_service = HttpBatchService::new(client, move |req| {
            let request_builder = Arc::clone(&http_request_builder);
            let future: BoxFuture<'static, Result<http::Request<Vec<u8>>, crate::Error>> =
                Box::pin(async move { request_builder.build_request(req).await });
            future
        });
        Self { batch_service }
    }
}

impl Service<HecLogsRequest> for HecLogsService {
    type Response = HecLogsResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: HecLogsRequest) -> Self::Future {
        let mut http_service = self.batch_service.clone();
        Box::pin(async move {
            http_service.ready().await?;
            // let batch_size = req.batch_size;
            // let byte_size = req.events_byte_size;
            let response = http_service.call(req).await?;
            let event_status = if response.status().is_success() {
                // TODO, use real metrics
                emit!(&EventsSent {
                    count: 1,
                    byte_size: 1,
                });
                EventStatus::Delivered
            } else if response.status().is_server_error() {
                EventStatus::Errored
            } else {
                EventStatus::Failed
            };

            Ok(HecLogsResponse { event_status })
        })
    }
}

#[derive(Clone)]
pub struct HecLogsRequest {
    pub body: Vec<u8>,
    finalizers: EventFinalizers,
}

impl ByteSizeOf for HecLogsRequest {
    fn allocated_bytes(&self) -> usize {
        self.body.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for HecLogsRequest {
    fn element_count(&self) -> usize {
        1
    }
}

impl Ackable for HecLogsRequest {
    fn ack_size(&self) -> usize {
        1
    }
}

impl Finalizable for HecLogsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

pub struct HecLogsResponse {
    event_status: EventStatus,
}

impl AsRef<EventStatus> for HecLogsResponse {
    fn as_ref(&self) -> &EventStatus {
        &self.event_status
    }
}

#[derive(Debug, Snafu)]
pub enum HecLogsError {
    #[snafu(display("Server responded with an error."))]
    ServerError,
    #[snafu(display("Failed to make HTTP(S) request: {}", error))]
    HttpError { error: crate::http::HttpError },
    #[snafu(display("Client sent a payload that is too large."))]
    PayloadTooLarge,
    #[snafu(display("Client request was not valid for unknown reasons."))]
    BadRequest,
}

#[derive(Debug, Default, Clone)]
pub struct HecLogsRetry;

impl RetryLogic for HecLogsRetry {
    type Error = HecLogsError;
    type Response = HecLogsResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        false
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, Derivative)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

impl Default for Encoding {
    fn default() -> Self {
        Self::Text
    }
}

pub struct HttpRequestBuilder {
    pub endpoint: String,
    pub token: String,
    pub content_encoding: Option<&'static str>,
}

impl HttpRequestBuilder {
    pub async fn build_request(
        &self,
        req: HecLogsRequest,
    ) -> Result<Request<Vec<u8>>, crate::Error> {
        let uri = build_uri(self.endpoint.as_str(), "/services/collector/event")
            .context(UriParseError)?;

        let mut builder = Request::post(uri)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Splunk {}", self.token.as_str()));

        if let Some(ce) = self.content_encoding {
            builder = builder.header("Content-Encoding", ce);
        }

        builder.body(req.body).map_err(Into::into)
    }
}

pub struct HecLogsRequestBuilder {
    pub compression: Compression,
    pub encoding: EncodingConfig<HecLogsEncoder>,
}

impl RequestBuilder<((), Vec<ProcessedEvent>)> for HecLogsRequestBuilder {
    type Metadata = (usize, EventFinalizers);
    type Events = Vec<ProcessedEvent>;
    // type Events = Vec<Event>;
    // type Events = Event;
    type Encoder = EncodingConfig<HecLogsEncoder>;
    type Payload = Vec<u8>;
    type Request = HecLogsRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        // &self.encoding.into()
        // &EncodingConfig::from(StandardEncodings::Json)
        &self.encoding
    }

    fn split_input(&self, input: ((), Vec<ProcessedEvent>)) -> (Self::Metadata, Self::Events) {
        let (_, mut events) = input;
        let finalizers = events.take_finalizers();
        // let finalizers = events.iter().map(|e| e.take_finalizers()).collect();

        ((events.len(), finalizers), events)
        // ((1, finalizers), events)
    }

    fn encode_events(&self, events: Self::Events) -> Result<Self::Payload, Self::Error> {
        println!("[HecLogsRequestBuilder::encode_events] {:?}", events);
        let mut payload = Vec::new();
        self.encoding.encode_input(events, &mut payload)?;
        Ok(payload)
        // Ok(self.encode_event(events).unwrap_or(vec![]))
        // Ok(events
        //     .into_iter()
        //     .filter_map(|e| self.encode_event(e))
        //     .flatten()
        //     .collect())
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        println!("[HecLogsRequestBuilder::build_request] {:?}", metadata);
        let (_, finalizers) = metadata;
        HecLogsRequest {
            body: payload,
            finalizers,
        }
    }
}
