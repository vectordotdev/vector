use std::{fmt, iter};

use async_trait::async_trait;
use futures_util::{StreamExt, future, stream::BoxStream};
use tower::Service;
use vector_core::{event::{Event, EventStatus, Metric, MetricValue}, sink::StreamSink};

use crate::{config::SinkContext, internal_events::SplunkInvalidMetricReceived, sinks::{splunk_hec::{common::render_template_string, metrics_new::encoder::{FieldMap, FieldValue}}, util::{SinkBuilderExt, encode_namespace, processed_event::ProcessedEvent}}, template::Template};

use super::request_builder::HecMetricsRequest;

pub struct HecMetricsSink<S> {
    context: SinkContext,
    service: S,
    pub sourcetype: Option<Template>,
    pub source: Option<Template>,
    pub index: Option<Template>,
    pub host: String,
    pub default_namespace: Option<String>,
}

#[async_trait]
impl<S> StreamSink for HecMetricsSink<S> 
where 
    S: Service<HecMetricsRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: AsRef<EventStatus> + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let sourcetype = self.sourcetype.as_ref();
        let source = self.source.as_ref();
        let index = self.index.as_ref();
        let host = self.host.as_ref();
        let default_namespace = self.default_namespace.as_deref();

        let sink = input
            .map(|event| event.into_metric())
            .filter_map(move |metric| future::ready(process_metric(metric, sourcetype, source, index, host, default_namespace)))
            .into_driver(self.service, self.context.acker());

        sink.run().await
    }
}

pub struct HecMetricsProcessedEventMetadata<'a> {
    sourcetype: Option<String>,
    source: Option<String>,
    index: Option<String>,
    host: Option<String>,
    timestamp: f64,
    fields: FieldMap<'a>,
}

impl<'a> HecMetricsProcessedEventMetadata<'a> {
    fn extract_metric_name(metric: &Metric, default_namespace: Option<&str>) -> String {
        encode_namespace(
            metric
                .namespace()
                .or_else(|| default_namespace),
            '.',
            metric.name(),
        )
    }

    fn extract_metric_value(metric: &Metric) -> Option<f64> {
        match *metric.value() {
            MetricValue::Counter { value } => Some(value),
            MetricValue::Gauge { value } => Some(value),
            _ => {
                emit!(&SplunkInvalidMetricReceived {
                    value: metric.value(),
                    kind: &metric.kind(),
                });
                None
            }
        }
    }

    fn extract_fields(metric: &'a Metric, templated_field_keys: Vec<String>, default_namespace: Option<&str>) -> Option<FieldMap<'a>> {
        let metric_name = Self::extract_metric_name(metric, default_namespace);
        let metric_value = Self::extract_metric_value(metric)?;

        Some(
            metric
                .tags()
                .into_iter()
                .flatten()
                .filter(|(k, _)| !templated_field_keys.contains(k))
                .map(|(k, v)| (k.as_str(), FieldValue::from(v.as_str())))
                .chain(iter::once(("metric_name", FieldValue::from(metric_name))))
                .chain(iter::once(("_value", FieldValue::from(metric_value))))
                .collect::<FieldMap>(),
        )
    }
}

pub type HecProcessedEvent<'a> = ProcessedEvent<Metric, HecMetricsProcessedEventMetadata<'a>>;

fn process_metric<'a>(
    metric: Metric,
    sourcetype: Option<&Template>,
    source: Option<&Template>,
    index: Option<&Template>,
    host_key: &str,
    default_namespace: Option<&str>,
) -> Option<HecProcessedEvent<'a>> {
    let templated_field_keys = 
        [index.as_ref(), source.as_ref(), sourcetype.as_ref()]
        .iter()
        .flatten()
        .filter_map(|t| t.get_fields())
        .flatten()
        .map(|f| f.replace("tags.", ""))
        .collect::<Vec<_>>();
    let fields = HecMetricsProcessedEventMetadata::extract_fields(&metric, templated_field_keys, default_namespace)?;

    let sourcetype =
        sourcetype.and_then(|sourcetype| render_template_string(sourcetype, &metric, "sourcetype"));
    let source = source.and_then(|source| render_template_string(source, &metric, "source"));
    let index = index.and_then(|index| render_template_string(index, &metric, "index"));
    let host = metric.tag_value(host_key);
    let timestamp = metric
        .timestamp()
        .unwrap_or_else(chrono::Utc::now)
        .timestamp_millis() as f64
        / 1000f64;

    let metadata = HecMetricsProcessedEventMetadata {
        sourcetype,
        source,
        index,
        host,
        timestamp,
        fields,
    };

    Some(HecProcessedEvent {
        event: metric,
        metadata,
    })
}
