use crate::sinks::util::{StreamSink, SinkBuilderExt, BatchSettings, Compression};
use futures::stream::BoxStream;
use crate::event::Event;
use vector_core::partition::Partitioner;
use std::num::NonZeroUsize;
use std::time::Duration;
use futures::StreamExt;
use crate::sinks::elasticsearch::request_builder::{ElasticsearchRequestBuilder, ProcessedEvent};
use crate::buffers::Acker;
use crate::sinks::elasticsearch::service::ElasticSearchService;
use crate::sinks::elasticsearch::{BulkAction, Encoding};
use crate::transforms::metric_to_log::MetricToLog;
use vector_core::stream::BatcherSettings;
use async_trait::async_trait;
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use rusoto_credential::AwsCredentials;

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct PartitionKey {
    index: String,
    bulk_action: BulkAction,
}

pub struct ElasticSearchPartitioner;

impl Partitioner for ElasticSearchPartitioner {
    type Item = ProcessedEvent;
    type Key = PartitionKey;

    fn partition(&self, item: &ProcessedEvent) -> Self::Key {
        //TODO: remove the allocation here?
        todo!()

    }
}

pub struct ElasticSearchSink {
    pub batch_settings: BatcherSettings,
    // batch_timeout: Duration,
    pub batch_size_bytes: Option<NonZeroUsize>,
    pub batch_size_events: NonZeroUsize,
    pub request_builder: ElasticsearchRequestBuilder,
    pub compression: Compression,
    pub service: ElasticSearchService,
    pub acker: Acker,
    pub metric_to_log: MetricToLog,
    pub encoding: EncodingConfigWithDefault<Encoding>,
}

impl ElasticSearchSink {
    pub async fn run_inner(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {

        let request_builder_concurrency_limit = NonZeroUsize::new(50);

        let sink = input
            // .filter_map(|event| async move {
            //     let log = match event {
            //         Event::Log(log) => Some(log),
            //         Event::Metric(metric) => self.metric_to_log.transform_one(metric),
            //     };
            // })
            .scan(self.metric_to_log, |metric_to_log, event| async move {
                Some(event)
            })
            .filter_map(|log|async move {
                let event:ProcessedEvent = todo!();
                Some(event)
            })
            .batched(ElasticSearchPartitioner,
                self.batch_settings,
            )
            .filter_map(|(_, batch)|async move {
                let aws_creds:Option<AwsCredentials> = todo!();
                Some(super::request_builder::Input{
                    aws_credentials: aws_creds,
                    events: batch
                })
            })
            .request_builder(
                request_builder_concurrency_limit,
                self.request_builder,
            ).filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build Elasticsearch request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}



#[async_trait]
impl StreamSink for ElasticSearchSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
