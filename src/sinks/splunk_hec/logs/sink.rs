use std::{fmt, num::NonZeroUsize, sync::Arc};

use async_trait::async_trait;
use futures_util::{stream::BoxStream, StreamExt};
use tower::Service;
use vector_core::{
    event::{Event, LogEvent, Value},
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
    ByteSizeOf,
};

use super::request_builder::HecLogsRequestBuilder;
use crate::{
    config::SinkContext,
    sinks::{
        splunk_hec::common::{render_template_string, request::HecRequest},
        util::{processed_event::ProcessedEvent, SinkBuilderExt},
    },
    template::Template,
};
use lookup::path;

pub struct HecLogsSink<S> {
    pub context: SinkContext,
    pub service: S,
    pub request_builder: HecLogsRequestBuilder,
    pub batch_settings: BatcherSettings,
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    pub index: Option<Template>,
    pub indexed_fields: Vec<String>,
    pub host: String,
    pub timestamp_nanos_key: Option<String>,
    pub timestamp_key: String,
}

impl<S> HecLogsSink<S>
where
    S: Service<HecRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let sourcetype = self.sourcetype.as_ref();
        let source = self.source.as_ref();
        let index = self.index.as_ref();
        let indexed_fields = self.indexed_fields.as_slice();
        let host = self.host.as_ref();
        let timestamp_nanos_key = self.timestamp_nanos_key.as_deref();
        let timestamp_key = self.timestamp_key.as_ref();

        let builder_limit = NonZeroUsize::new(64);
        let sink = input
            .map(move |event| {
                process_log(
                    event,
                    sourcetype,
                    source,
                    index,
                    host,
                    indexed_fields,
                    timestamp_nanos_key,
                    timestamp_key,
                )
            })
            .batched_partitioned(EventPartitioner::default(), self.batch_settings)
            .request_builder(builder_limit, self.request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build HEC Logs request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.context.acker());

        sink.run().await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for HecLogsSink<S>
where
    S: Service<HecRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[derive(Default)]
struct EventPartitioner;

impl Partitioner for EventPartitioner {
    type Item = HecProcessedEvent;
    type Key = Option<Arc<str>>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.event.metadata().splunk_hec_token().clone()
    }
}

#[derive(PartialEq, Default, Clone, Debug)]
pub struct HecLogsProcessedEventMetadata {
    pub event_byte_size: usize,
    pub sourcetype: Option<String>,
    pub source: Option<String>,
    pub index: Option<String>,
    pub host: Option<Value>,
    pub timestamp: Option<f64>,
    pub fields: LogEvent,
}

impl ByteSizeOf for HecLogsProcessedEventMetadata {
    fn allocated_bytes(&self) -> usize {
        self.sourcetype.allocated_bytes()
            + self.source.allocated_bytes()
            + self.index.allocated_bytes()
            + self.host.allocated_bytes()
            + self.fields.allocated_bytes()
    }
}

pub type HecProcessedEvent = ProcessedEvent<LogEvent, HecLogsProcessedEventMetadata>;

pub fn process_log(
    event: Event,
    sourcetype: Option<&Template>,
    source: Option<&Template>,
    index: Option<&Template>,
    host_key: &str,
    indexed_fields: &[String],
    timestamp_nanos_key: Option<&str>,
    timestamp_key: &str,
) -> HecProcessedEvent {
    let event_byte_size = event.size_of();
    let mut log = event.into_log();

    let sourcetype =
        sourcetype.and_then(|sourcetype| render_template_string(sourcetype, &log, "sourcetype"));

    let source = source.and_then(|source| render_template_string(source, &log, "source"));

    let index = index.and_then(|index| render_template_string(index, &log, "index"));

    let host = log.get(host_key).cloned();

    let timestamp = if timestamp_key.is_empty() {
        info!("timestamp_key is empty. Allowing splunk to set timestamp.");
        None
    } else {
        match log.remove(timestamp_key) {
            Some(Value::Timestamp(ts)) => {
                // set nanos in log if valid timestamp in event and timestamp_nanos_key is configured
                if let Some(key) = timestamp_nanos_key {
                    log.try_insert(path!(key), ts.timestamp_subsec_nanos() % 1_000_000);
                }

                Some((ts.timestamp_millis() as f64) / 1000f64)
            }
            Some(value) => {
                warn!(
                    "timestamp_key is not valid timestamp: type is {}. Allowing splunk to set timestamp.",
                    value.kind_str()
                );
                None
            }
            None => {
                warn!("No timestamp was not found. Allowing splunk to set timestamp.");
                None
            }
        }
    };

    let fields = indexed_fields
        .iter()
        .filter_map(|field| log.get(field.as_str()).map(|value| (field, value.clone())))
        .collect::<LogEvent>();

    let metadata = HecLogsProcessedEventMetadata {
        event_byte_size,
        sourcetype,
        source,
        index,
        host,
        timestamp,
        fields,
    };

    ProcessedEvent {
        event: log,
        metadata,
    }
}
