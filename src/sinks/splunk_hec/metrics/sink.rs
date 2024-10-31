use std::{fmt, sync::Arc};

use serde::Serialize;
use vector_lib::event::{Metric, MetricValue};
use vrl::path::OwnedValuePath;

use super::request_builder::HecMetricsRequestBuilder;
use crate::{
    internal_events::SplunkInvalidMetricReceivedError,
    sinks::{
        prelude::*,
        splunk_hec::common::{render_template_string, request::HecRequest},
        util::{encode_namespace, processed_event::ProcessedEvent},
    },
};

pub struct HecMetricsSink<S> {
    pub service: S,
    pub batch_settings: BatcherSettings,
    pub request_builder: HecMetricsRequestBuilder,
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    pub index: Option<Template>,
    pub host_key: Option<OwnedValuePath>,
    pub default_namespace: Option<String>,
}

impl<S> HecMetricsSink<S>
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
        let host_key = self.host_key.as_ref();
        let default_namespace = self.default_namespace.as_deref();
        let batch_settings = self.batch_settings;

        input
            .map(|event| (event.size_of(), event.into_metric()))
            .filter_map(move |(event_byte_size, metric)| {
                future::ready(process_metric(
                    metric,
                    event_byte_size,
                    sourcetype,
                    source,
                    index,
                    host_key,
                    default_namespace,
                ))
            })
            .batched_partitioned(EventPartitioner, || batch_settings.as_byte_size_config())
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build HEC Metrics request: {:?}.", e);
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
impl<S> StreamSink<Event> for HecMetricsSink<S>
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
        item.event.metadata().splunk_hec_token()
    }
}

#[derive(Serialize)]
pub struct HecMetricsProcessedEventMetadata {
    pub event_byte_size: usize,
    pub sourcetype: Option<String>,
    pub source: Option<String>,
    pub index: Option<String>,
    pub host: Option<String>,
    pub metric_name: String,
    pub metric_value: f64,
    pub templated_field_keys: Vec<String>,
}

impl ByteSizeOf for HecMetricsProcessedEventMetadata {
    fn allocated_bytes(&self) -> usize {
        self.sourcetype.allocated_bytes()
            + self.source.allocated_bytes()
            + self.index.allocated_bytes()
            + self.host.allocated_bytes()
            + self.metric_name.allocated_bytes()
            + self.templated_field_keys.allocated_bytes()
    }
}

impl HecMetricsProcessedEventMetadata {
    fn extract_metric_name(metric: &Metric, default_namespace: Option<&str>) -> String {
        encode_namespace(metric.namespace().or(default_namespace), '.', metric.name())
    }

    fn extract_metric_value(metric: &Metric) -> Option<f64> {
        match *metric.value() {
            MetricValue::Counter { value } => Some(value),
            MetricValue::Gauge { value } => Some(value),
            _ => {
                emit!(SplunkInvalidMetricReceivedError {
                    value: metric.value(),
                    kind: &metric.kind(),
                    error: "Metric kind not supported.".into(),
                });
                None
            }
        }
    }
}

pub type HecProcessedEvent = ProcessedEvent<Metric, HecMetricsProcessedEventMetadata>;

pub fn process_metric(
    metric: Metric,
    event_byte_size: usize,
    sourcetype: Option<&Template>,
    source: Option<&Template>,
    index: Option<&Template>,
    host_key: Option<&OwnedValuePath>,
    default_namespace: Option<&str>,
) -> Option<HecProcessedEvent> {
    let templated_field_keys = [index.as_ref(), source.as_ref(), sourcetype.as_ref()]
        .iter()
        .flatten()
        .filter_map(|t| t.get_fields())
        .flatten()
        .map(|f| f.replace("tags.", ""))
        .collect::<Vec<_>>();
    let metric_name =
        HecMetricsProcessedEventMetadata::extract_metric_name(&metric, default_namespace);
    let metric_value = HecMetricsProcessedEventMetadata::extract_metric_value(&metric)?;

    let sourcetype =
        sourcetype.and_then(|sourcetype| render_template_string(sourcetype, &metric, "sourcetype"));
    let source = source.and_then(|source| render_template_string(source, &metric, "source"));
    let index = index.and_then(|index| render_template_string(index, &metric, "index"));
    let host = host_key.and_then(|key| metric.tag_value(key.to_string().as_str()));

    let metadata = HecMetricsProcessedEventMetadata {
        event_byte_size,
        sourcetype,
        source,
        index,
        host,
        metric_name,
        metric_value,
        templated_field_keys,
    };

    Some(HecProcessedEvent {
        event: metric,
        metadata,
    })
}

impl EventCount for HecProcessedEvent {
    fn event_count(&self) -> usize {
        // A HecProcessedEvent is mapped one-to-one with an event.
        1
    }
}
