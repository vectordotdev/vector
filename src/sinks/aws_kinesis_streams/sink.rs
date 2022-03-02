use std::num::NonZeroUsize;

use async_trait::async_trait;
use futures::{future, stream::BoxStream, StreamExt};
use rand::random;
use tower::util::BoxService;
use vector_core::{buffers::Acker, stream::BatcherSettings};

use crate::{
    event::{Event, LogEvent},
    sinks::{
        aws_kinesis_streams::{
            request_builder::{KinesisRequest, KinesisRequestBuilder},
            service::KinesisResponse,
        },
        util::{processed_event::ProcessedEvent, SinkBuilderExt, StreamSink},
    },
    Error,
};

pub type KinesisProcessedEvent = ProcessedEvent<LogEvent, KinesisMetadata>;

pub struct KinesisMetadata {
    pub partition_key: String,
}

pub struct KinesisSink {
    pub batch_settings: BatcherSettings,
    pub acker: Acker,
    pub service: BoxService<Vec<KinesisRequest>, KinesisResponse, Error>,
    pub request_builder: KinesisRequestBuilder,
    pub partition_key_field: Option<String>,
}

impl KinesisSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder_concurrency_limit = NonZeroUsize::new(50);

        let partition_key_field = self.partition_key_field.clone();
        let sink = input
            .map(|event| {
                // Panic: This sink only accepts Logs, so this should never panic
                event.into_log()
            })
            .filter_map(move |log| future::ready(process_log(log, &partition_key_field)))
            .request_builder(request_builder_concurrency_limit, self.request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build Kinesis Stream request: {:?}.", e);
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
impl StreamSink<Event> for KinesisSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

pub fn process_log(
    log: LogEvent,
    partition_key_field: &Option<String>,
) -> Option<KinesisProcessedEvent> {
    let partition_key = if let Some(partition_key_field) = partition_key_field {
        if let Some(v) = log.get(&partition_key_field) {
            v.to_string_lossy()
        } else {
            warn!(
                message = "Partition key does not exist; dropping event.",
                %partition_key_field,
                internal_log_rate_secs = 30,
            );
            return None;
        }
    } else {
        gen_partition_key()
    };
    let partition_key = if partition_key.len() >= 256 {
        partition_key[..256].to_string()
    } else {
        partition_key
    };

    Some(KinesisProcessedEvent {
        event: log,
        metadata: KinesisMetadata { partition_key },
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
