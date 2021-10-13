use crate::sinks::util::{StreamSink, SinkBuilderExt, BatchSettings, Compression};
use futures::stream::BoxStream;
use crate::event::Event;
use vector_core::partition::{Partitioner, NullPartitioner};
use std::num::NonZeroUsize;
use std::time::Duration;
use futures::StreamExt;
use crate::sinks::elasticsearch::request_builder::ElasticsearchRequestBuilder;
use crate::buffers::Acker;
use crate::sinks::elasticsearch::service::ElasticSearchService;
use crate::sinks::elasticsearch::{BulkAction, Encoding};
use crate::transforms::metric_to_log::MetricToLog;
use vector_core::stream::BatcherSettings;
use async_trait::async_trait;
use crate::sinks::util::encoding::EncodingConfigWithDefault;
use rusoto_credential::AwsCredentials;
use futures::future;
use crate::sinks::elasticsearch::encoder::ProcessedEvent;
use vector_core::ByteSizeOf;

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct PartitionKey {
    pub index: String,
    pub bulk_action: BulkAction,
}

// pub struct ElasticSearchPartitioner;
//
// impl Partitioner for ElasticSearchPartitioner {
//     type Item = ProcessedEvent;
//     type Key = PartitionKey;
//
//     fn partition(&self, item: &ProcessedEvent) -> Self::Key {
//
//         // remove the allocation here?
//         PartitionKey {
//             index: item.index.clone(),
//             bulk_action: BulkAction::Index
//         }
//
//     }
// }

pub struct BatchedEvents {
    pub key: PartitionKey,
    pub events: Vec<ProcessedEvent>
}

impl ByteSizeOf for BatchedEvents {
    fn allocated_bytes(&self) -> usize {
        todo!()
    }
}

pub struct ElasticSearchSink {
    pub batch_settings: BatcherSettings,
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
    pub async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {

        let request_builder_concurrency_limit = NonZeroUsize::new(50);

        let sink = input
            .scan(self.metric_to_log, |metric_to_log, event| {
                future::ready(Some(match event {
                    Event::Metric(metric) => metric_to_log.transform_one(metric),
                    Event::Log(log) => Some(log)
                }))
            })
            .filter_map(|x|async move {x})
            .filter_map(|log|async move {
                // if let Some(cfg) = self.mode.as_data_stream_config() {
                //     cfg.sync_fields(&mut event.log);
                //     cfg.remap_timestamp(&mut event.log);
                // };
                // maybe_set_id(
                //     self.id_key.as_ref(),
                //     action.pointer_mut(event.bulk_action.as_json_pointer()).unwrap(),
                //     &mut event.log,
                // );
                let event:ProcessedEvent = todo!();
                Some(event)
            })
            // .batched(ElasticSearchPartitioner,
            //     self.batch_settings,
            // )
            // .map(|(key, events)| {
            //     BatchedEvents { key, events }
            // })
            .batched(NullPartitioner::new(), self.batch_settings)
            .filter_map(|(partition, batch)|async move {
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
