use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use prost::Message;
use tower::Service;
use vector_lib::request_metadata::GroupedCountByteSize;
use vector_lib::stream::{batcher::data::BatchReduce, BatcherSettings, DriverResponse};
use vector_lib::{config::telemetry, ByteSizeOf, EstimatedJsonEncodedSizeOf};

use super::service::VectorRequest;
use crate::{
    event::{proto::EventWrapper, Event, EventFinalizers, Finalizable},
    proto::vector as proto_vector,
    sinks::util::{metadata::RequestMetadataBuilder, SinkBuilderExt, StreamSink},
};

/// Data for a single event.
struct EventData {
    byte_size: usize,
    json_byte_size: GroupedCountByteSize,
    finalizers: EventFinalizers,
    wrapper: EventWrapper,
}

/// Temporary struct to collect events during batching.
#[derive(Clone)]
struct EventCollection {
    pub finalizers: EventFinalizers,
    pub events: Vec<EventWrapper>,
    pub events_byte_size: usize,
    pub events_json_byte_size: GroupedCountByteSize,
}

impl Default for EventCollection {
    fn default() -> Self {
        Self {
            finalizers: Default::default(),
            events: Default::default(),
            events_byte_size: Default::default(),
            events_json_byte_size: telemetry().create_request_count_byte_size(),
        }
    }
}

pub struct VectorSink<S> {
    pub batch_settings: BatcherSettings,
    pub service: S,
}

impl<S> VectorSink<S>
where
    S: Service<VectorRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .map(|mut event| {
                let mut byte_size = telemetry().create_request_count_byte_size();
                byte_size.add_event(&event, event.estimated_json_encoded_size_of());

                EventData {
                    byte_size: event.size_of(),
                    json_byte_size: byte_size,
                    finalizers: event.take_finalizers(),
                    wrapper: EventWrapper::from(event),
                }
            })
            .batched(self.batch_settings.as_reducer_config(
                |data: &EventData| data.wrapper.encoded_len(),
                BatchReduce::new(|event_collection: &mut EventCollection, item: EventData| {
                    event_collection.finalizers.merge(item.finalizers);
                    event_collection.events.push(item.wrapper);
                    event_collection.events_byte_size += item.byte_size;
                    event_collection.events_json_byte_size += item.json_byte_size;
                }),
            ))
            .map(|event_collection| {
                let builder = RequestMetadataBuilder::new(
                    event_collection.events.len(),
                    event_collection.events_byte_size,
                    event_collection.events_json_byte_size,
                );

                let encoded_events = proto_vector::PushEventsRequest {
                    events: event_collection.events,
                };

                let byte_size = encoded_events.encoded_len();
                let bytes_len =
                    NonZeroUsize::new(byte_size).expect("payload should never be zero length");

                VectorRequest {
                    finalizers: event_collection.finalizers,
                    metadata: builder.with_request_size(bytes_len),
                    request: encoded_events,
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for VectorSink<S>
where
    S: Service<VectorRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
