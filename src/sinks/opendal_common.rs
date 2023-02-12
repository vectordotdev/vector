//! opendal_common provide real sink supports for all opendal based services.
//!
//! # TODO
//!
//! opendal service now only support very basic sink features. To make it
//! useful, we need to add the following features:
//!
//! - Batch events
//! - Error handling
//! - Limitation
//! - Compression
//! - KeyPartition

use crate::event::EventFinalizers;
use crate::sinks::util::encoding::write_all;
use crate::sinks::util::encoding::Encoder;
use crate::sinks::util::metadata::RequestMetadataBuilder;
use crate::sinks::util::{request_builder::EncodeResult, Compression};
use crate::sinks::BoxFuture;
use crate::{
    event::Event,
    internal_events::SinkRequestBuildError,
    sinks::util::{RequestBuilder, SinkBuilderExt},
};
use bytes::Bytes;
use futures::{stream::BoxStream, StreamExt};
use opendal::Operator;
use snafu::Snafu;
use std::task::Poll;
use tower::Service;
use vector_common::finalization::{EventStatus, Finalizable};
use vector_common::request_metadata::MetaDescriptive;
use vector_common::request_metadata::RequestMetadata;
use vector_core::internal_event::CountByteSize;
use vector_core::sink::StreamSink;
use vector_core::stream::DriverResponse;

pub struct OpendalSink {
    op: Operator,
}

impl OpendalSink {
    pub fn new(op: Operator) -> Self {
        Self { op }
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for OpendalSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

impl OpendalSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .request_builder(
                None,
                OpendalRequestBuilder {
                    encoder: OpendalEncoder,
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
            .into_driver(OpendalService::new(self.op.clone()))
            .run()
            .await
    }
}

/// TODO: we should implment batch encoder.
#[derive(Clone)]
struct OpendalEncoder;

impl Encoder<Event> for OpendalEncoder {
    fn encode_input(
        &self,
        input: Event,
        writer: &mut dyn std::io::Write,
    ) -> std::io::Result<usize> {
        let event = serde_json::to_string(&input).unwrap();
        write_all(writer, 1, event.as_bytes()).map(|()| event.len())
    }
}

#[derive(Debug, Clone)]
pub struct OpendalService {
    op: Operator,
}

impl OpendalService {
    pub const fn new(op: Operator) -> OpendalService {
        OpendalService { op }
    }
}

pub struct OpendalRequest {
    pub path: String,
    pub payload: Bytes,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl MetaDescriptive for OpendalRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.metadata
    }
}

impl Finalizable for OpendalRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

struct OpendalRequestBuilder {
    encoder: OpendalEncoder,
}

impl RequestBuilder<Event> for OpendalRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Event;
    type Encoder = OpendalEncoder;
    type Payload = Bytes;
    type Request = OpendalRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut input: Event,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let finalizers = input.take_finalizers();
        let metadata_builder = RequestMetadataBuilder::from_events(&input);
        (finalizers, metadata_builder, input)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        OpendalRequest {
            // TODO: path should be construct by key partition like s3 and gcp.
            path: uuid::Uuid::new_v4().to_string(),
            finalizers: metadata,
            payload: payload.into_payload(),
            metadata: request_metadata,
        }
    }
}

#[derive(Debug)]
pub struct OpendalResponse {
    byte_size: usize,
}

impl DriverResponse for OpendalResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        // (events count, byte size)
        CountByteSize(1, self.byte_size)
    }
}

impl Service<OpendalRequest> for OpendalService {
    type Response = OpendalResponse;
    type Error = opendal::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: OpendalRequest) -> Self::Future {
        let byte_size = request.payload.len();
        let op = self.op.clone();

        Box::pin(async move {
            let result = op.object(&request.path).write(request.payload).await;
            result.map(|_| OpendalResponse { byte_size })
        })
    }
}

#[derive(Debug, Snafu)]
pub enum OpendalError {
    #[snafu(display("Failed to call OpenDAL: {}", source))]
    OpenDAL { source: opendal::Error },
}

impl From<opendal::Error> for OpendalError {
    fn from(source: opendal::Error) -> Self {
        Self::OpenDAL { source }
    }
}
