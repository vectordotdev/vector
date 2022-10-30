use std::{fmt, num::NonZeroUsize};

use async_trait::async_trait;
use futures::{future, stream::BoxStream, StreamExt};
use rand::random;
use tower::Service;
use vector_common::{
    finalization::{EventFinalizers, Finalizable},
    request_metadata::{MetaDescriptive, RequestMetadata},
};
use vector_core::{
    event::LogEvent,
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};

use super::request_builder::{KinesisRequest, KinesisRequestBuilder};
use crate::{
    event::Event,
    internal_events::SinkRequestBuildError,
    sinks::util::{processed_event::ProcessedEvent, SinkBuilderExt},
};

#[derive(Debug, Clone)]
struct KinesisFirehoseRetryLogic;

pub type KinesisProcessedEvent = ProcessedEvent<LogEvent, KinesisKey>;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct KinesisKey {
    pub partition_key: String,
}

pub struct KinesisSink<S> {
    pub batch_settings: BatcherSettings,
    pub service: S,
    pub request_builder: KinesisRequestBuilder,
}

impl<S> KinesisSink<S>
where
    S: Service<BatchKinesisRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder_concurrency_limit = NonZeroUsize::new(50);

        input
            .filter_map(|event| {
                // Panic: This sink only accepts Logs, so this should never panic
                let log = event.into_log();
                let processed = process_log(log);
                future::ready(processed)
            })
            .request_builder(request_builder_concurrency_limit, self.request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .batched_partitioned(KinesisPartitioner, self.batch_settings)
            .map(|(key, events)| {
                let metadata =
                    RequestMetadata::from_batch(events.iter().map(|req| req.get_metadata()));
                BatchKinesisRequest {
                    key,
                    events,
                    metadata,
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for KinesisSink<S>
where
    S: Service<BatchKinesisRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

pub(crate) fn process_log(log: LogEvent) -> Option<KinesisProcessedEvent> {
    let partition_key = gen_partition_key();

    let partition_key = if partition_key.len() >= 256 {
        partition_key[..256].to_string()
    } else {
        partition_key
    };

    Some(KinesisProcessedEvent {
        event: log,
        metadata: KinesisKey { partition_key },
    })
}

fn gen_partition_key() -> String {
    random::<[char; 16]>()
        .iter()
        .fold(String::new(), |mut s, c| {
            s.push(*c);
            s
        })
}

#[derive(Clone)]
pub struct BatchKinesisRequest {
    pub key: KinesisKey,
    pub events: Vec<KinesisRequest>,
    metadata: RequestMetadata,
}

impl Finalizable for BatchKinesisRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.events.take_finalizers()
    }
}

impl MetaDescriptive for BatchKinesisRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.metadata
    }
}

struct KinesisPartitioner;

impl Partitioner for KinesisPartitioner {
    type Item = KinesisRequest;
    type Key = KinesisKey;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.key.clone()
    }
}
