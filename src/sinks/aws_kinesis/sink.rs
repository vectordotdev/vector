use std::{borrow::Cow, fmt::Debug, marker::PhantomData};

use rand::random;
use vector_lib::lookup::lookup_v2::ConfigValuePath;
use vrl::path::PathPrefix;

use crate::{
    internal_events::{AwsKinesisStreamNoPartitionKeyError, SinkRequestBuildError},
    sinks::{
        prelude::*,
        util::{processed_event::ProcessedEvent, StreamSink},
    },
};

use super::{
    record::Record,
    request_builder::{KinesisRequest, KinesisRequestBuilder},
};

pub type KinesisProcessedEvent = ProcessedEvent<LogEvent, KinesisKey>;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct KinesisKey {
    pub partition_key: String,
}

#[derive(Clone)]
pub struct KinesisSink<S, R> {
    pub batch_settings: BatcherSettings,
    pub service: S,
    pub request_builder: KinesisRequestBuilder<R>,
    pub partition_key_field: Option<ConfigValuePath>,
    pub _phantom: PhantomData<R>,
}

impl<S, R> KinesisSink<S, R>
where
    S: Service<BatchKinesisRequest<R>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
    R: Record + Send + Sync + Unpin + Clone + 'static,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;

        input
            .filter_map(|event| {
                // Panic: This sink only accepts Logs, so this should never panic
                let log = event.into_log();
                let processed = process_log(log, self.partition_key_field.as_ref());

                future::ready(processed)
            })
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .batched(batch_settings.as_byte_size_config())
            .map(|events| {
                let metadata = RequestMetadata::from_batch(
                    events.iter().map(|req| req.get_metadata().clone()),
                );
                BatchKinesisRequest { events, metadata }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait]
impl<S, R> StreamSink<Event> for KinesisSink<S, R>
where
    S: Service<BatchKinesisRequest<R>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
    R: Record + Send + Sync + Unpin + Clone + 'static,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Returns a `KinesisProcessedEvent` containing the unmodified log event + metadata consisting of
/// the partition key. The partition key is either generated from the provided partition_key_field
/// or is generated randomly.
///
/// If the provided partition_key_field was not found in the log, `Error` `EventsDropped` internal
/// events are emitted and None is returned.
pub(crate) fn process_log(
    log: LogEvent,
    partition_key_field: Option<&ConfigValuePath>,
) -> Option<KinesisProcessedEvent> {
    let partition_key = if let Some(partition_key_field) = partition_key_field {
        if let Some(v) = log.get((PathPrefix::Event, partition_key_field)) {
            v.to_string_lossy()
        } else {
            emit!(AwsKinesisStreamNoPartitionKeyError {
                partition_key_field: partition_key_field.0.to_string().as_str()
            });
            return None;
        }
    } else {
        Cow::Owned(gen_partition_key())
    };
    let partition_key = if partition_key.len() >= 256 {
        partition_key[..256].to_string()
    } else {
        partition_key.into_owned()
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

pub struct BatchKinesisRequest<R>
where
    R: Record + Clone,
{
    pub events: Vec<KinesisRequest<R>>,
    metadata: RequestMetadata,
}

impl<R> Clone for BatchKinesisRequest<R>
where
    R: Record + Clone,
{
    fn clone(&self) -> Self {
        Self {
            events: self.events.to_vec(),
            metadata: self.metadata.clone(),
        }
    }
}

impl<R> Finalizable for BatchKinesisRequest<R>
where
    R: Record + Clone,
{
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.events.take_finalizers()
    }
}

impl<R> MetaDescriptive for BatchKinesisRequest<R>
where
    R: Record + Clone,
{
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}
