//! opendal_common provide real sink supports for all opendal based services.
//!
//! # TODO
//!
//! opendal service now only support very basic sink features. To make it
//! useful, we need to add the following features:
//!
//! - Error handling
//! - Limitation

use std::{fmt, task::Poll};

use bytes::Bytes;
use opendal::Operator;
use snafu::Snafu;
use tracing::Instrument;
use vector_lib::codecs::encoding::Framer;

use crate::sinks::{prelude::*, util::partitioner::KeyPartitioner};

/// OpenDalSink provides generic a service upon OpenDAL.
///
/// # Notes
///
/// OpenDAL based service only need to provide a `<Service>Config`, and
/// implement `build_processor` like `WebHdfs` does.
pub struct OpenDalSink<Svc> {
    service: Svc,
    request_builder: OpenDalRequestBuilder,
    partitioner: KeyPartitioner,
    batcher_settings: BatcherSettings,
}

impl<Svc> OpenDalSink<Svc> {
    /// Build a new OpenDalSink via given input
    pub const fn new(
        service: Svc,
        request_builder: OpenDalRequestBuilder,
        partitioner: KeyPartitioner,
        batcher_settings: BatcherSettings,
    ) -> Self {
        Self {
            service,
            request_builder,
            partitioner,
            batcher_settings,
        }
    }
}

#[async_trait::async_trait]
impl<Svc> StreamSink<Event> for OpenDalSink<Svc>
where
    Svc: Service<OpenDalRequest> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

impl<Svc> OpenDalSink<Svc>
where
    Svc: Service<OpenDalRequest> + Send + 'static,
    Svc::Future: Send + 'static,
    Svc::Response: DriverResponse + Send + 'static,
    Svc::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let partitioner = self.partitioner;
        let settings = self.batcher_settings;

        let request_builder = self.request_builder;

        input
            .batched_partitioned(partitioner, || settings.as_byte_size_config())
            .filter_map(|(key, batch)| async move {
                // We don't need to emit an error here if the event is dropped since this will occur if the template
                // couldn't be rendered during the partitioning. A `TemplateRenderingError` is already emitted when
                // that occurs.
                key.map(move |k| (k, batch))
            })
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            // TODO: set protocol with services scheme instead hardcoded file
            .protocol("file")
            .run()
            .await
    }
}

/// OpenDalService is just a simple wrapper of `opendal::Operator` to
/// implement traits we needed.
#[derive(Debug, Clone)]
pub struct OpenDalService {
    op: Operator,
}

impl OpenDalService {
    pub const fn new(op: Operator) -> OpenDalService {
        OpenDalService { op }
    }
}

/// OpenDalRequest is request will be handled by opendal services.
///
/// It will carry all information that opendal needed, like payload and
/// metadata.
#[derive(Clone)]
pub struct OpenDalRequest {
    pub payload: Bytes,
    pub metadata: OpenDalMetadata,
    pub request_metadata: RequestMetadata,
}

impl MetaDescriptive for OpenDalRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

impl Finalizable for OpenDalRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

/// OpenDalMetadata carries metadata that opendal service needed to write.
#[derive(Clone)]
pub struct OpenDalMetadata {
    pub partition_key: String,
    pub count: usize,
    pub byte_size: JsonSize,
    pub finalizers: EventFinalizers,
}

/// OpenDalRequestBuilder will collect and encode input events to build a
/// valid [`OpenDalRequest`].
pub struct OpenDalRequestBuilder {
    pub encoder: (Transformer, Encoder<Framer>),
    pub compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for OpenDalRequestBuilder {
    type Metadata = OpenDalMetadata;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = OpenDalRequest;
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
        let opendal_metadata = OpenDalMetadata {
            partition_key,
            count: events.len(),
            byte_size: events.estimated_json_encoded_size_of(),
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

        OpenDalRequest {
            metadata,
            payload: payload.into_payload(),
            request_metadata,
        }
    }
}

/// OpenDalResponse is the response returned by OpenDAL services.
#[derive(Debug)]
pub struct OpenDalResponse {
    pub events_byte_size: GroupedCountByteSize,
    pub byte_size: usize,
}

impl DriverResponse for OpenDalResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}

impl Service<OpenDalRequest> for OpenDalService {
    type Response = OpenDalResponse;
    type Error = opendal::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, request: OpenDalRequest) -> Self::Future {
        let byte_size = request.payload.len();
        let op = self.op.clone();

        Box::pin(async move {
            let result = op
                .write(&request.metadata.partition_key, request.payload)
                .in_current_span()
                .await;
            result.map(|_| OpenDalResponse {
                events_byte_size: request
                    .request_metadata
                    .into_events_estimated_json_encoded_byte_size(),
                byte_size,
            })
        })
    }
}

/// OpenDalError is the error returned by opendal services.
///
/// # TODO
///
/// We need to provide more context about opendal errors.
#[derive(Debug, Snafu)]
pub enum OpenDalError {
    #[snafu(display("Failed to call OpenDal: {}", source))]
    OpenDal { source: opendal::Error },
}

impl From<opendal::Error> for OpenDalError {
    fn from(source: opendal::Error) -> Self {
        Self::OpenDal { source }
    }
}
