use std::num::NonZeroUsize;
use async_graphql::futures_util::stream::BoxStream;
use futures::StreamExt;
use rusoto_core::RusotoError;
use rusoto_firehose::{PutRecordBatchError, PutRecordBatchOutput};
use tower::util::BoxService;
use vector_core::partition::NullPartitioner;
use vector_core::sink::StreamSink;
use vector_core::stream::BatcherSettings;
use crate::buffers::Acker;
use crate::event::Event;
use crate::{Error, rusoto};
use crate::sinks::aws_kinesis_firehose::request_builder::{KinesisRequest, KinesisRequestBuilder};
use crate::sinks::aws_kinesis_firehose::service::{KinesisResponse, KinesisService};
use crate::sinks::util::retries::RetryLogic;
use crate::sinks::util::SinkBuilderExt;
use async_trait::async_trait;

#[derive(Debug, Clone)]
struct KinesisFirehoseRetryLogic;




pub struct KinesisSink {
    pub batch_settings: BatcherSettings,
    pub service: BoxService<Vec<KinesisRequest>, KinesisResponse, Error>,
    pub acker: Acker,
    pub request_builder: KinesisRequestBuilder
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
            .batched(NullPartitioner::new(), self.batch_settings)
            .map(|(_, batch)| batch)
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}

impl StreamSink for KinesisSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

