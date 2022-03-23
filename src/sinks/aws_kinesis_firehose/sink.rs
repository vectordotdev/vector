use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use tower::Service;
use vector_core::{
    buffers::Acker,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};

use crate::{
    event::Event,
    sinks::{
        aws_kinesis_firehose::request_builder::{KinesisRequest, KinesisRequestBuilder},
        util::SinkBuilderExt,
    },
};

#[derive(Debug, Clone)]
struct KinesisFirehoseRetryLogic;

pub struct KinesisSink<S> {
    pub batch_settings: BatcherSettings,
    pub service: S,
    pub acker: Acker,
    pub request_builder: KinesisRequestBuilder,
}

impl<S> KinesisSink<S>
where
    S: Service<Vec<KinesisRequest>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder_concurrency_limit = NonZeroUsize::new(50);

        let sink = input
            .map(|event| {
                // Panic: This sink only accepts Logs, so this should never panic
                event.into_log()
            })
            .request_builder(request_builder_concurrency_limit, self.request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build Kinesis Firehose request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .batched(self.batch_settings.into_byte_size_config())
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for KinesisSink<S>
where
    S: Service<Vec<KinesisRequest>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
