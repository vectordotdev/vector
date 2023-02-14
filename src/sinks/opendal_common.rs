//! opendal_common provide real sink supports for all opendal based services.
//!
//! # TODO
//!
//! opendal service now only support very basic sink features. To make it
//! useful, we need to add the following features:
//!
//! - Error handling
//! - Limitation

use std::{num::NonZeroUsize, task::Poll};

use bytes::Bytes;
use codecs::encoding::Framer;
use futures::{stream::BoxStream, StreamExt};
use opendal::Operator;
use snafu::Snafu;
use tower::Service;
use tracing::Instrument;
use vector_common::{
    finalization::{EventStatus, Finalizable},
    request_metadata::{MetaDescriptive, RequestMetadata},
};
use vector_core::{
    internal_event::CountByteSize,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
    ByteSizeOf,
};

use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, EventFinalizers},
    internal_events::SinkRequestBuildError,
    sinks::{
        util::{
            metadata::RequestMetadataBuilder, partitioner::KeyPartitioner,
            request_builder::EncodeResult, Compression, RequestBuilder, SinkBuilderExt,
        },
        BoxFuture,
    },
};

pub struct OpenDALSink {
    op: Operator,
    request_builder: OpenDALRequestBuilder,
    partitioner: KeyPartitioner,
    batcher_settings: BatcherSettings,
}

impl OpenDALSink {
    pub fn new(
        op: Operator,
        request_builder: OpenDALRequestBuilder,
        partitioner: KeyPartitioner,
        batcher_settings: BatcherSettings,
    ) -> Self {
        Self {
            op,
            request_builder,
            partitioner,
            batcher_settings,
        }
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for OpenDALSink {
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

impl OpenDALSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let partitioner = self.partitioner;
        let settings = self.batcher_settings;

        let builder_limit = NonZeroUsize::new(64);
        let request_builder = self.request_builder;

        input
            .batched_partitioned(partitioner, settings)
            .filter_map(|(key, batch)| async move {
                // We don't need to emit an error here if the event is dropped since this will occur if the template
                // couldn't be rendered during the partitioning. A `TemplateRenderingError` is already emitted when
                // that occurs.
                key.map(move |k| (k, batch))
            })
            .request_builder(builder_limit, request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(OpenDALService::new(self.op.clone()))
            // TODO: set protocl with services scheme instead hardcoded file
            .protocol("file")
            .run()
            .await
    }
}

#[derive(Debug, Clone)]
pub struct OpenDALService {
    op: Operator,
}

impl OpenDALService {
    pub const fn new(op: Operator) -> OpenDALService {
        OpenDALService { op }
    }
}

#[derive(Clone)]
pub struct OpenDALRequest {
    pub payload: Bytes,
    pub metadata: OpenDALMetadata,
    pub request_metadata: RequestMetadata,
}

impl MetaDescriptive for OpenDALRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.request_metadata
    }
}

impl Finalizable for OpenDALRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

#[derive(Clone)]
pub struct OpenDALMetadata {
    pub partition_key: String,
    pub count: usize,
    pub byte_size: usize,
    pub finalizers: EventFinalizers,
}

pub struct OpenDALRequestBuilder {
    pub encoder: (Transformer, Encoder<Framer>),
    pub compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for OpenDALRequestBuilder {
    type Metadata = OpenDALMetadata;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = OpenDALRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (String, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (partition_key, mut events) = input;
        let finalizers = events.take_finalizers();
        let opendal_metadata = OpenDALMetadata {
            partition_key,
            count: events.len(),
            byte_size: events.size_of(),
            finalizers,
        };

        let builder = RequestMetadataBuilder::from_events(&events);

        (opendal_metadata, builder, events)
    }

    fn build_request(
        &self,
        mut metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        // TODO: we can support time format later.
        let name = uuid::Uuid::new_v4().to_string();
        let extension = self.compression.extension();

        metadata.partition_key = format!("{}{}.{}", metadata.partition_key, name, extension);

        OpenDALRequest {
            metadata,
            payload: payload.into_payload(),
            request_metadata: request_metadata,
        }
    }
}

#[derive(Debug)]
pub struct OpenDALResponse {
    pub count: usize,
    pub events_byte_size: usize,
    pub byte_size: usize,
}

impl DriverResponse for OpenDALResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(self.count, self.events_byte_size)
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}

impl Service<OpenDALRequest> for OpenDALService {
    type Response = OpenDALResponse;
    type Error = opendal::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, request: OpenDALRequest) -> Self::Future {
        let byte_size = request.payload.len();
        let op = self.op.clone();

        Box::pin(async move {
            let result = op
                .object(&request.metadata.partition_key.as_str())
                .write(request.payload)
                .in_current_span()
                .await;
            result.map(|_| OpenDALResponse {
                count: request.metadata.count,
                events_byte_size: request.metadata.byte_size,
                byte_size,
            })
        })
    }
}

#[derive(Debug, Snafu)]
pub enum OpenDALError {
    #[snafu(display("Failed to call OpenDAL: {}", source))]
    OpenDAL { source: opendal::Error },
}

impl From<opendal::Error> for OpenDALError {
    fn from(source: opendal::Error) -> Self {
        Self::OpenDAL { source }
    }
}
