#![allow(unused_imports)]
use std::task::Poll;

use super::util::encoding::{write_all, Encoder};
use super::util::metadata::RequestMetadataBuilder;
use super::util::{Compression, RequestBuilder, SinkBuilderExt};
use super::Healthcheck;
use crate::config::{GenerateConfig, SinkConfig, SinkContext};
use crate::http::HttpClient;
use bytes::Bytes;
use futures::future::BoxFuture;
use futures::{stream::BoxStream, StreamExt};
use snafu::Snafu;
use vector_common::finalization::EventFinalizers;
use vector_common::{
    finalization::{EventStatus, Finalizable},
    internal_event::{BytesSent, EventsSent},
};
use vector_config::configurable_component;
use vector_core::tls::TlsSettings;
use vector_core::{
    config::{AcknowledgementsConfig, Input},
    event::Event,
    sink::{StreamSink, VectorSink},
    EstimatedJsonEncodedSizeOf,
};

#[configurable_component(sink("basic"))]
#[derive(Clone, Debug)]
/// A basic sink that dumps its output to stdout.
pub struct BasicConfig {
    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for BasicConfig {
    fn generate_config() -> toml::Value {
        toml::from_str("").unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for BasicConfig {
    async fn build(&self, _cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let healthcheck = Box::pin(async move { Ok(()) });
        let sink = VectorSink::from_event_streamsink(BasicSink::new(&self));

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::log()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

struct BasicResponse;

struct BasicService {
    endpoint: String,
    client: HttpClient,
}

impl tower::Service<Vec<u8>> for BasicService {
    type Response = BasicResponse;

    type Error = &'static str;

    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Vec<u8>) -> Self::Future {
        let body = hyper::Body::from(request);
        let req = http::Request::post("http:localhost:5678")
            .header("Content-Type", "application/json")
            .body(body)
            .unwrap();

        let mut client = self.client.clone();

        Box::pin(async move {
            match client.call(req).await {
                Ok(response) => {
                    if response.status().is_success() {
                        Ok(BasicResponse)
                    } else {
                        Err("received error response")
                    }
                }
                Err(_error) => Err("oops"),
            }
        })
    }
}

#[derive(Debug, Clone)]
struct BasicSink {
    endpoint: String,
    client: HttpClient,
}

impl BasicSink {
    pub fn new(config: &BasicConfig) -> Self {
        let tls = TlsSettings::from_options(&None).unwrap();
        let client = HttpClient::new(tls, &Default::default()).unwrap();
        let endpoint = "http://localhost:5678".to_string();

        Self { client, endpoint }
    }
}

#[derive(Clone)]
struct BasicEncoder;

impl Encoder<Event> for BasicEncoder {
    fn encode_input(
        &self,
        input: Event,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<usize> {
        let event = serde_json::to_string(&input).unwrap();
        write_all(writer, 1, event.as_bytes()).map(|()| event.len())
    }
}

#[derive(Clone)]
struct BasicRequestBuilder {
    encoder: BasicEncoder,
}

#[derive(Debug, Snafu)]
pub enum RequestBuildError {
    #[snafu(display("An error occurred."))]
    PayloadTooBig,
    #[snafu(display("Failed to build payload with error: {}", error))]
    Io { error: std::io::Error },
}

impl From<std::io::Error> for RequestBuildError {
    fn from(error: std::io::Error) -> RequestBuildError {
        RequestBuildError::Io { error }
    }
}

#[derive(Clone)]
struct BasicRequest {
    payload: Bytes,
    finalizers: EventFinalizers,
}

impl Finalizable for BasicRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.finalizers.take_finalizers()
    }
}

impl RequestBuilder<Event> for BasicRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Event;
    type Encoder = BasicEncoder;
    type Payload = Bytes;
    type Request = BasicRequest;
    type Error = RequestBuildError;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: Event,
    ) -> (
        Self::Metadata,
        super::util::metadata::RequestMetadataBuilder,
        Self::Events,
    ) {
        let finalizers = input.take_finalizers();
        // TODO - these need proper numbers.
        let metadata_builder = RequestMetadataBuilder::new(1, 1, 1);
        (finalizers, metadata_builder, input)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        _request_metadata: vector_common::request_metadata::RequestMetadata,
        payload: super::util::request_builder::EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let finalizers = metadata;

        BasicRequest {
            finalizers,
            payload: payload.into_payload(),
        }
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for BasicSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

impl BasicSink {
    async fn run_inner(self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        let service = tower::ServiceBuilder::new().service(BasicService {
            client: self.client.clone(),
            endpoint: self.endpoint.clone(),
        });

        let sink = input
            .request_builder(
                None,
                BasicRequestBuilder {
                    encoder: BasicEncoder,
                },
            )
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(service);

        sink.run().await

        // while let Some(event) = input.next().await {
        //     let bytes = format!("{:#?}", event);
        //     println!("{}", bytes);

        //     emit!(BytesSent {
        //         byte_size: bytes.len(),
        //         protocol: "none".into()
        //     });

        //     let event_byte_size = event.estimated_json_encoded_size_of();
        //     emit!(EventsSent {
        //         count: 1,
        //         byte_size: event_byte_size,
        //         output: None,
        //     })
        // }
        // Ok(())
    }
}
