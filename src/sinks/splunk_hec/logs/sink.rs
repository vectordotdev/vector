use std::{fmt, num::NonZeroUsize, sync::Arc};

use async_trait::async_trait;
use futures_util::{future, stream::BoxStream, StreamExt};
use tower::Service;
use vector_core::{
    config::log_schema,
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

        let builder_limit = NonZeroUsize::new(64);
        let sink = input
            .map(|event| (event.size_of(), event.into_log()))
            .filter_map(move |(event_byte_size, log)| {
                future::ready(process_log(
                    log,
                    event_byte_size,
                    sourcetype,
                    source,
                    index,
                    host,
                    indexed_fields,
                    timestamp_nanos_key,
                ))
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
    pub timestamp: f64,
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
    mut log: LogEvent,
    event_byte_size: usize,
    sourcetype: Option<&Template>,
    source: Option<&Template>,
    index: Option<&Template>,
    host_key: &str,
    indexed_fields: &[String],
    timestamp_nanos_key: Option<&str>,
) -> Option<HecProcessedEvent> {
    let sourcetype =
        sourcetype.and_then(|sourcetype| render_template_string(sourcetype, &log, "sourcetype"));

    let source = source.and_then(|source| render_template_string(source, &log, "source"));

    let index = index.and_then(|index| render_template_string(index, &log, "index"));

    let host = log.get(host_key).cloned();

    let timestamp = match log.remove(log_schema().timestamp_key()) {
        Some(Value::Timestamp(ts)) => ts,
        _ => chrono::Utc::now(),
    };

    if let Some(key) = timestamp_nanos_key {
        log.try_insert_flat(key, timestamp.timestamp_subsec_nanos() % 1_000_000);
    }

    let timestamp = (timestamp.timestamp_millis() as f64) / 1000f64;

    let fields = indexed_fields
        .iter()
        .filter_map(|field| log.get(field).map(|value| (field, value.clone())))
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

    Some(ProcessedEvent {
        event: log,
        metadata,
    })
}
