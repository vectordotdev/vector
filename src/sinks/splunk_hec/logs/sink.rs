use std::{fmt, sync::Arc};

use serde::Serialize;

use super::request_builder::HecLogsRequestBuilder;
use crate::{
    internal_events::SplunkEventTimestampInvalidType,
    internal_events::SplunkEventTimestampMissing,
    sinks::{
        prelude::*,
        splunk_hec::common::{
            render_template_string, request::HecRequest, EndpointTarget, INDEX_FIELD,
            SOURCETYPE_FIELD, SOURCE_FIELD,
        },
        util::processed_event::ProcessedEvent,
    },
};
use vector_lib::lookup::{event_path, OwnedTargetPath, OwnedValuePath, PathPrefix};

pub struct HecLogsSink<S> {
    pub context: SinkContext,
    pub service: S,
    pub request_builder: HecLogsRequestBuilder,
    pub batch_settings: BatcherSettings,
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    pub index: Option<Template>,
    pub indexed_fields: Vec<OwnedValuePath>,
    pub host_key: Option<OwnedTargetPath>,
    pub timestamp_nanos_key: Option<String>,
    pub timestamp_key: Option<OwnedTargetPath>,
    pub endpoint_target: EndpointTarget,
}

pub struct HecLogData<'a> {
    pub sourcetype: Option<&'a Template>,
    pub source: Option<&'a Template>,
    pub index: Option<&'a Template>,
    pub indexed_fields: &'a [OwnedValuePath],
    pub host_key: Option<OwnedTargetPath>,
    pub timestamp_nanos_key: Option<&'a String>,
    pub timestamp_key: Option<OwnedTargetPath>,
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
        let data = HecLogData {
            sourcetype: self.sourcetype.as_ref(),
            source: self.source.as_ref(),
            index: self.index.as_ref(),
            indexed_fields: self.indexed_fields.as_slice(),
            host_key: self.host_key.clone(),
            timestamp_nanos_key: self.timestamp_nanos_key.as_ref(),
            timestamp_key: self.timestamp_key.clone(),
            endpoint_target: self.endpoint_target,
        };
        let batch_settings = self.batch_settings;

        input
            .map(move |event| process_log(event, &data))
            .batched_partitioned(
                if self.endpoint_target == EndpointTarget::Raw {
                    // We only need to partition by the metadata fields for the raw endpoint since those fields
                    // are sent via query parameters in the request.
                    EventPartitioner::new(
                        self.sourcetype.clone(),
                        self.source.clone(),
                        self.index.clone(),
                        self.host_key.clone(),
                    )
                } else {
                    EventPartitioner::new(None, None, None, None)
                },
                || batch_settings.as_byte_size_config(),
            )
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build HEC Logs request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .run()
            .await
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

#[derive(Clone, Debug, PartialEq, Hash, Eq)]
pub(super) struct Partitioned {
    pub(super) token: Option<Arc<str>>,
    pub(super) source: Option<String>,
    pub(super) sourcetype: Option<String>,
    pub(super) index: Option<String>,
    pub(super) host: Option<String>,
}

#[derive(Default)]
struct EventPartitioner {
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    pub index: Option<Template>,
    pub host_key: Option<OwnedTargetPath>,
}

impl EventPartitioner {
    const fn new(
        sourcetype: Option<Template>,
        source: Option<Template>,
        index: Option<Template>,
        host_key: Option<OwnedTargetPath>,
    ) -> Self {
        Self {
            sourcetype,
            source,
            index,
            host_key,
        }
    }
}

impl Partitioner for EventPartitioner {
    type Item = HecProcessedEvent;
    type Key = Option<Partitioned>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let emit_err = |error, field| {
            emit!(TemplateRenderingError {
                error,
                field: Some(field),
                drop_event: false,
            })
        };

        let source = self.source.as_ref().and_then(|source| {
            source
                .render_string(&item.event)
                .map_err(|error| emit_err(error, SOURCE_FIELD))
                .ok()
        });

        let sourcetype = self.sourcetype.as_ref().and_then(|sourcetype| {
            sourcetype
                .render_string(&item.event)
                .map_err(|error| emit_err(error, SOURCETYPE_FIELD))
                .ok()
        });

        let index = self.index.as_ref().and_then(|index| {
            index
                .render_string(&item.event)
                .map_err(|error| emit_err(error, INDEX_FIELD))
                .ok()
        });

        let host = self
            .host_key
            .as_ref()
            .and_then(|host_key| item.event.get(host_key))
            .and_then(|value| value.as_str().map(|s| s.to_string()));

        Some(Partitioned {
            token: item.event.metadata().splunk_hec_token(),
            source,
            sourcetype,
            index,
            host,
        })
    }
}

#[derive(PartialEq, Default, Clone, Debug, Serialize)]
pub struct HecLogsProcessedEventMetadata {
    pub event_byte_size: usize,
    pub sourcetype: Option<String>,
    pub source: Option<String>,
    pub index: Option<String>,
    pub host: Option<Value>,
    pub timestamp: Option<f64>,
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

pub fn process_log(event: Event, data: &HecLogData) -> HecProcessedEvent {
    let event_byte_size = event.size_of();
    let mut log = event.into_log();

    let sourcetype = data
        .sourcetype
        .and_then(|sourcetype| render_template_string(sourcetype, &log, SOURCETYPE_FIELD));

    let source = data
        .source
        .and_then(|source| render_template_string(source, &log, SOURCE_FIELD));

    let index = data
        .index
        .and_then(|index| render_template_string(index, &log, INDEX_FIELD));

    let host = data.host_key.as_ref().and_then(|key| log.get(key)).cloned();

    let timestamp = data.timestamp_key.as_ref().and_then(|timestamp_key| {
        match log.remove(timestamp_key) {
            Some(Value::Timestamp(ts)) => {
                // set nanos in log if valid timestamp in event and timestamp_nanos_key is configured
                if let Some(key) = data.timestamp_nanos_key {
                    log.try_insert(event_path!(key), ts.timestamp_subsec_nanos() % 1_000_000);
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
    });

    let fields = data
        .indexed_fields
        .iter()
        .filter_map(|field| {
            log.get((PathPrefix::Event, field))
                .map(|value| (field.to_string(), value.clone()))
        })
        .collect::<LogEvent>();

    let metadata = HecLogsProcessedEventMetadata {
        event_byte_size,
        sourcetype,
        source,
        index,
        host,
        timestamp,
        fields,
        endpoint_target: data.endpoint_target,
    };

    ProcessedEvent {
        event: log,
        metadata,
    }
}

impl EventCount for HecProcessedEvent {
    fn event_count(&self) -> usize {
        // A HecProcessedEvent is mapped one-to-one with an event.
        1
    }
}
