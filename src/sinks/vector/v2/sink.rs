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
use async_trait::async_trait;
use snafu::Snafu;
use std::future;
use crate::event::proto::EventWrapper;
use vector_core::ByteSizeOf;

pub struct VectorSink {
    pub batch_settings: BatcherSettings,
    pub service: BoxService<Vec<EventWrapperWrapper>, VectorResponse, Error>,
    pub acker: Acker,
}



// so we can impl ByteSizeOf for EventWrapper
pub struct EventWrapperWrapper {
    inner: EventWrapper
}

impl ByteSizeOf for EventWrapperWrapper {
    fn allocated_bytes(&self) -> usize {
        inner.compute_size()
    }
}

impl VectorSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .then(|event| future::ready(EventWrapperWrapper {inner: EventWrapper::from(event)}))
            .batched(NullPartitioner::new(), self.batch_settings)
            .map(|(_, batch)| batch)
            .into_driver(self.service, self.acker)
            .run().await
    }
}

#[async_trait]
impl StreamSink for VectorSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
