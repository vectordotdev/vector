use crate::sinks::util::{StreamSink, SinkBuilderExt};
use crate::event::Event;
use std::num::NonZeroUsize;
use futures::StreamExt;
use futures::stream::BoxStream;
use vector_core::partition::NullPartitioner;
use vector_core::stream::BatcherSettings;
use tower::util::BoxService;
use crate::Error;
use vector_core::buffers::Acker;
use crate::sinks::vector::v2::service::VectorResponse;

pub struct VectorSink {
    pub batch_settings: BatcherSettings,
    pub service: BoxService<Vec<EventWrapper>, VectorResponse, Error>,
    pub acker: Acker,
}

impl VectorSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .map(|event| future::ready(EventWrapper::from(event)))
            .batched(NullPartitioner::new(), self.batch_settings)
            .map(|(_, batch)| batch)
            .into_driver(self.service, self.acker)
            .run().await
    }
}

impl StreamSink for VectorSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
