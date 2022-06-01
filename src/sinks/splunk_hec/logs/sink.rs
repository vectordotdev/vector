use std::{collections::HashMap, fmt, num::NonZeroUsize, sync::Arc};

use async_trait::async_trait;
use futures_util::{stream::BoxStream, StreamExt};
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
    internal_events::TemplateRenderingError,
    sinks::{
        splunk_hec::common::{render_template_string, request::HecRequest, EndpointTarget},
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
    pub metadata: HashMap<String, Template>,
    pub endpoint_target: EndpointTarget,
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
        let metadata = self.metadata.clone();
        let endpoint_target = self.endpoint_target;

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
                    metadata.clone(),
                    endpoint_target,
                )
            })
            .batched_partitioned(
                EventPartitioner::new(self.metadata.clone()),
                self.batch_settings,
            )
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

#[derive(Clone, Debug, Eq)]
pub(super) struct Partitioned {
    pub(super) token: Option<Arc<str>>,
    pub(super) metadata: HashMap<String, String>,
}

impl Partitioned {
    pub(super) fn into_parts(self) -> (Option<Arc<str>>, HashMap<String, String>) {
        (self.token, self.metadata)
    }
}

impl PartialEq for Partitioned {
    fn eq(&self, other: &Self) -> bool {
        self.token == other.token && self.metadata == other.metadata
    }
}

impl std::hash::Hash for Partitioned {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.token.hash(state);

        for (key, value) in &self.metadata {
            key.hash(state);
            value.hash(state);
        }
    }
}

#[derive(Default)]
struct EventPartitioner {
    metadata: HashMap<String, Template>,
}

impl EventPartitioner {
    const fn new(metadata: HashMap<String, Template>) -> Self {
        Self { metadata }
    }
}

impl Partitioner for EventPartitioner {
    type Item = HecProcessedEvent;
    type Key = Option<Partitioned>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        self.metadata
            .iter()
            .map(|(key, value)| {
                let res = value
                    .render_string(&item.event)
                    .map_err(|error| {
                        emit!(TemplateRenderingError {
                            error,
                            field: Some(key.as_str()),
                            drop_event: false,
                        })
                    })
                    .ok()
                    .map(|rendered| (key.clone(), rendered));

                res
            })
            .collect::<Option<_>>()
            .map(|metadata| Partitioned {
                token: item.event.metadata().splunk_hec_token().clone(),
                metadata,
            })
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
    pub endpoint_target: EndpointTarget,
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
    metadata: HashMap<String, Template>,
    endpoint_target: EndpointTarget,
) -> HecProcessedEvent {
    let event_byte_size = event.size_of();
    let mut log = event.into_log();

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
        endpoint_target,
    };

    ProcessedEvent {
        event: log,
        metadata,
    }
}
