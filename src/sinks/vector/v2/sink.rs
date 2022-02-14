use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use prost::Message;
use tower::util::BoxService;
use vector_core::{buffers::Acker, stream::BatcherSettings, ByteSizeOf};

use crate::{
    event::{proto::EventWrapper, Event, EventFinalizers, Finalizable},
    sinks::{
        util::{SinkBuilderExt, StreamSink},
        vector::v2::service::{VectorRequest, VectorResponse},
    },
    Error,
};

struct EventData {
    byte_size: usize,
    finalizers: EventFinalizers,
    wrapper: EventWrapper,
}

pub struct VectorSink {
    pub batch_settings: BatcherSettings,
    pub service: BoxService<VectorRequest, VectorResponse, Error>,
    pub acker: Acker,
}

impl VectorSink {
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
impl StreamSink<Event> for VectorSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
