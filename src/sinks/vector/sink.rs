use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use prost::Message;
use tower::Service;
use vector_core::{
    stream::{BatcherSettings, DriverResponse},
    ByteSizeOf,
};

use super::service::VectorRequest;
use crate::{
    event::{proto::EventWrapper, Event, EventFinalizers, Finalizable},
    proto::vector as proto_vector,
    sinks::util::{metadata::RequestMetadataBuilder, SinkBuilderExt, StreamSink},
};

struct EventData {
    byte_size: usize,
    finalizers: EventFinalizers,
    wrapper: EventWrapper,
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
            .map(|mut event| EventData {
                byte_size: event.size_of(),
                finalizers: event.take_finalizers(),
                wrapper: EventWrapper::from(event),
            })
            .batched(self.batch_settings.into_reducer_config(
                |data: &EventData| data.wrapper.encoded_len(),
                |req: &mut VectorRequest, item: EventData| {
                    req.finalizers.merge(item.finalizers);
                    req.events.push(item.wrapper);
                    req.events_byte_size += item.byte_size;
                },
            ))
            .map(|mut v_request| {
                let builder = RequestMetadataBuilder::new(
                    v_request.events.len(),
                    v_request.events_byte_size,
                    v_request.events_byte_size, // this is fine as it isn't being used
                );

                let encoded_events = proto_vector::PushEventsRequest {
                    events: v_request.events.clone(),
                };

                let byte_size = encoded_events.encoded_len();
                let bytes_len =
                    NonZeroUsize::new(byte_size).expect("payload should never be zero length");

                v_request.metadata = builder.with_request_size(bytes_len);
                v_request.request = encoded_events;

                v_request
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
