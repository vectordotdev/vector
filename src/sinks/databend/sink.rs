use std::io;
use std::num::NonZeroUsize;

use bytes::Bytes;
use codecs::encoding::Framer;
use futures_util::{stream::BoxStream, StreamExt};
use vector_common::finalization::{EventFinalizers, Finalizable};
use vector_common::request_metadata::RequestMetadata;
use vector_core::event::Event;
use vector_core::sink::StreamSink;
use vector_core::stream::BatcherSettings;

use crate::{
    codecs::{Encoder, Transformer},
    internal_events::SinkRequestBuildError,
    sinks::util::{
        metadata::RequestMetadataBuilder, request_builder::EncodeResult, service::Svc, Compression,
        RequestBuilder, SinkBuilderExt,
    },
};

use super::service::{DatabendRequest, DatabendRetryLogic, DatabendService};

#[derive(Clone)]
pub struct DatabendRequestBuilder {
    compression: Compression,
    encoder: (Transformer, Encoder<Framer>),
}

impl DatabendRequestBuilder {
    pub const fn new(compression: Compression, encoder: (Transformer, Encoder<Framer>)) -> Self {
        Self {
            compression,
            encoder,
        }
    }
}

impl RequestBuilder<Vec<Event>> for DatabendRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = DatabendRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let mut events = input;
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, events)
    }

    fn build_request(
        &self,
        finalizers: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        DatabendRequest {
            finalizers,
            data: payload.into_payload(),
            metadata,
        }
    }
}

pub struct DatabendSink {
    batch_settings: BatcherSettings,
    request_builder: DatabendRequestBuilder,
    service: Svc<DatabendService, DatabendRetryLogic>,
}

impl DatabendSink {
    pub(super) const fn new(
        batch_settings: BatcherSettings,
        request_builder: DatabendRequestBuilder,
        service: Svc<DatabendService, DatabendRetryLogic>,
    ) -> Self {
        Self {
            batch_settings,
            request_builder,
            service,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let builder_limit = NonZeroUsize::new(64);
        input
            .batched(self.batch_settings.into_byte_size_config())
            .request_builder(builder_limit, self.request_builder)
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
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for DatabendSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
