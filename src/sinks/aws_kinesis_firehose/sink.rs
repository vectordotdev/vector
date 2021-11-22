use futures::StreamExt;
use std::num::NonZeroUsize;
use vector_core::buffers::Acker;

use crate::event::Event;
use crate::sinks::aws_kinesis_firehose::request_builder::{KinesisRequest, KinesisRequestBuilder};
use crate::sinks::aws_kinesis_firehose::service::KinesisResponse;
use crate::Error;
use futures::stream::BoxStream;
use tower::util::BoxService;
use vector_core::sink::StreamSink;
use vector_core::stream::{batcher, BatcherSettings};

use crate::sinks::util::SinkBuilderExt;
use async_trait::async_trait;

#[derive(Debug, Clone)]
struct KinesisFirehoseRetryLogic;

pub struct KinesisSink {
    pub batch_settings: BatcherSettings,
    pub service: BoxService<Vec<KinesisRequest>, KinesisResponse, Error>,
    pub acker: Acker,
    pub request_builder: KinesisRequestBuilder,
}

impl KinesisSink {
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
            .batched(batcher::config::byte_size_of_vec(self.batch_settings))
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}

#[async_trait]
impl StreamSink for KinesisSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
