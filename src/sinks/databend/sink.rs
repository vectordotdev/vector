use std::num::NonZeroUsize;

use bytes::{BufMut, Bytes, BytesMut};
use futures_util::{stream::BoxStream, StreamExt};
use vector_common::finalization::{EventFinalizers, Finalizable};
use vector_core::event::Event;
use vector_core::sink::StreamSink;
use vector_core::stream::BatcherSettings;
use vector_core::ByteSizeOf;

use crate::sinks::util::{metadata::RequestMetadataBuilder, service::Svc, SinkBuilderExt};

use super::{
    event_encoder::DatabendEventEncoder,
    service::{DatabendRequest, DatabendRetryLogic, DatabendService},
};

/// Data for a single event.
pub(crate) struct EventData {
    byte_size: usize,
    finalizers: EventFinalizers,
    data: Bytes,
}

/// Temporary struct to collect events during batching.
#[derive(Clone, Default)]
pub(crate) struct EventCollection {
    pub finalizers: EventFinalizers,
    pub data: BytesMut,
    pub count: usize,
    pub events_byte_size: usize,
}

pub struct DatabendSink {
    encoder: DatabendEventEncoder,
    batch_settings: BatcherSettings,
    service: Svc<DatabendService, DatabendRetryLogic>,
}

impl DatabendSink {
    pub(super) const fn new(
        encoder: DatabendEventEncoder,
        batch_settings: BatcherSettings,
        service: Svc<DatabendService, DatabendRetryLogic>,
    ) -> Self {
        Self {
            encoder,
            batch_settings,
            service,
        }
    }

    async fn run_inner(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .map(|mut event| EventData {
                byte_size: event.size_of(),
                finalizers: event.take_finalizers(),
                data: self.encoder.encode_event(event).into(),
            })
            .batched(self.batch_settings.into_reducer_config(
                |data: &EventData| data.data.len(),
                |event_collection: &mut EventCollection, item: EventData| {
                    event_collection.finalizers.merge(item.finalizers);
                    event_collection.data.put(item.data);
                    event_collection.events_byte_size += item.byte_size;
                    event_collection.count += 1;
                },
            ))
            .map(|event_collection| {
                let builder = RequestMetadataBuilder::new(
                    event_collection.count,
                    event_collection.events_byte_size,
                    event_collection.events_byte_size, // this is fine as it isn't being used
                );
                let data_len = NonZeroUsize::new(event_collection.data.len())
                    .expect("payload should never be zero length");
                DatabendRequest {
                    data: event_collection.data.freeze(),
                    finalizers: event_collection.finalizers,
                    metadata: builder.with_request_size(data_len),
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
