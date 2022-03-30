use std::fmt;

use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use prost::Message;
use tower::Service;
use vector_core::{
    buffers::Acker,
    stream::{BatcherSettings, DriverResponse},
    ByteSizeOf,
};

use crate::{
    event::{proto::EventWrapper, Event, EventFinalizers, Finalizable},
    sinks::{
        util::{SinkBuilderExt, StreamSink},
        vector::v2::service::VectorRequest,
    },
};

struct EventData {
    byte_size: usize,
    finalizers: EventFinalizers,
    wrapper: EventWrapper,
}

pub struct VectorSink<S> {
    pub batch_settings: BatcherSettings,
    pub service: S,
    pub acker: Acker,
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
                    req.events_byte_size += item.byte_size;
                    req.finalizers.merge(item.finalizers);
                    req.events.push(item.wrapper);
                },
            ))
            .into_driver(self.service, self.acker)
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
