use std::{collections::HashMap, fmt, num::NonZeroUsize, sync::Arc};

use async_trait::async_trait;
use codecs::encoding::SerializerConfig;
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
    internal_events::SplunkEventTimestampInvalidType,
    internal_events::SplunkEventTimestampMissing,
    internal_events::TemplateRenderingError,
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
    pub splunk_metadata: HashMap<String, Template>,
    pub encoding: SerializerConfig,
}

pub struct HecLogData<'a> {
    pub sourcetype: Option<&'a Template>,
    pub source: Option<&'a Template>,
    pub index: Option<&'a Template>,
    pub indexed_fields: &'a [String],
    pub host_key: &'a str,
    pub timestamp_nanos_key: Option<&'a String>,
    pub timestamp_key: &'a str,
}

impl<S> HecLogsSink<S>
where
    S: Service<HecRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let builder_limit = NonZeroUsize::new(64);

        let data = HecLogData {
            sourcetype: self.sourcetype.as_ref(),
            source: self.source.as_ref(),
            index: self.index.as_ref(),
            indexed_fields: self.indexed_fields.as_slice(),
            host_key: self.host.as_ref(),
            timestamp_nanos_key: self.timestamp_nanos_key.as_ref(),
            timestamp_key: self.timestamp_key.as_ref(),
        };

        let sink = input
            .map(move |event| process_log(event, &data))
            .batched_partitioned(
                // With the text encoding we need to batch on index, sourcetype and source
                // as those fields are sent as metadata in the url query parameters.
                // The event encoding sends them as part of the event.
                match self.encoding {
                    SerializerConfig::Json | SerializerConfig::NativeJson => {
                        EventPartitioner::new(None, None, None, self.splunk_metadata.clone())
                    }
                    _ => EventPartitioner::new(
                        self.index.as_ref(),
                        self.sourcetype.as_ref(),
                        self.source.as_ref(),
                        self.splunk_metadata.clone(),
                    ),
                },
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
    pub(super) splunk_metadata: HashMap<String, String>,
}

impl Partitioned {
    #[allow(clippy::missing_const_for_fn)]
    pub(super) fn into_parts(self) -> (Option<Arc<str>>, HashMap<String, String>) {
        (self.token, self.splunk_metadata)
    }
}

impl PartialEq for Partitioned {
    fn eq(&self, other: &Self) -> bool {
        self.token == other.token && self.splunk_metadata == other.splunk_metadata
    }
}

impl std::hash::Hash for Partitioned {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.token.hash(state);

        for (key, value) in &self.splunk_metadata {
            key.hash(state);
            value.hash(state);
        }
    }
}

#[derive(Default)]
struct EventPartitioner {
    splunk_metadata: HashMap<String, Template>,
}

impl EventPartitioner {
    fn new(
        index: Option<&Template>,
        sourcetype: Option<&Template>,
        source: Option<&Template>,
        mut splunk_metadata: HashMap<String, Template>,
    ) -> Self {
        if let Some(index) = index {
            splunk_metadata.insert("index".to_string(), index.clone());
        }

        if let Some(sourcetype) = sourcetype {
            splunk_metadata.insert("sourcetype".to_string(), sourcetype.clone());
        }

        if let Some(source) = source {
            splunk_metadata.insert("source".to_string(), source.clone());
        }

        Self { splunk_metadata }
    }
}

impl Partitioner for EventPartitioner {
    type Item = HecProcessedEvent;
    type Key = Option<Partitioned>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        self.splunk_metadata
            .iter()
            .map(|(key, value)| {
                value
                    .render_string(&item.event)
                    .map_err(|error| {
                        emit!(TemplateRenderingError {
                            error,
                            field: Some(key.as_str()),
                            drop_event: false,
                        })
                    })
                    .ok()
                    .map(|rendered| (key.clone(), rendered))
            })
            .collect::<Option<_>>()
            .map(|splunk_metadata| Partitioned {
                token: item.event.metadata().splunk_hec_token(),
                splunk_metadata,
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

pub fn process_log(event: Event, data: &HecLogData) -> HecProcessedEvent {
    let event_byte_size = event.size_of();
    let mut log = event.into_log();

    let sourcetype = data
        .sourcetype
        .and_then(|sourcetype| render_template_string(sourcetype, &log, "sourcetype"));

    let source = data
        .source
        .and_then(|source| render_template_string(source, &log, "source"));

    let index = data
        .index
        .and_then(|index| render_template_string(index, &log, "index"));

    let host = log.get(data.host_key).cloned();

    let timestamp = if data.timestamp_key.is_empty() {
        None
    } else {
        match log.remove(data.timestamp_key) {
            Some(Value::Timestamp(ts)) => {
                // set nanos in log if valid timestamp in event and timestamp_nanos_key is configured
                if let Some(key) = data.timestamp_nanos_key {
                    log.try_insert(path!(key), ts.timestamp_subsec_nanos() % 1_000_000);
                }
                Some((ts.timestamp_millis() as f64) / 1000f64)
            }
            Some(value) => {
                emit!(SplunkEventTimestampInvalidType {
                    r#type: value.kind_str()
                });
                None
            }
            None => {
                emit!(SplunkEventTimestampMissing {});
                None
            }
        }
    };

    let fields = data
        .indexed_fields
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
