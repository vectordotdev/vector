use std::fmt;

use vector_lib::{
    byte_size_of::ByteSizeOf,
    event::Metric,
    internal_event::emit,
    stream::batcher::{data::BatchData, limiter::ByteSizeOfItemSize},
};

use super::{
    PartitionKey, PrometheusMetricNormalize,
    request_builder::{RemoteWriteEncoder, RemoteWriteRequest, RemoteWriteRequestBuilder},
};
use crate::sinks::{prelude::*, util::buffer::metrics::MetricSet};

pub(super) struct RemoteWriteMetric {
    pub(super) metric: Metric,
    tenant_id: Option<String>,
}

impl Finalizable for RemoteWriteMetric {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.metric.take_finalizers()
    }
}

impl GetEventCountTags for RemoteWriteMetric {
    fn get_tags(&self) -> TaggedEventsSent {
        self.metric.get_tags()
    }
}

impl EstimatedJsonEncodedSizeOf for RemoteWriteMetric {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.metric.estimated_json_encoded_size_of()
    }
}

impl ByteSizeOf for RemoteWriteMetric {
    fn allocated_bytes(&self) -> usize {
        self.metric.allocated_bytes()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PrometheusRemoteWriteDefaultBatchSettings;

impl SinkBatchSettings for PrometheusRemoteWriteDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1_000);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

pub(super) struct PrometheusTenantIdPartitioner;

impl Partitioner for PrometheusTenantIdPartitioner {
    type Item = RemoteWriteMetric;
    type Key = PartitionKey;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        PartitionKey {
            tenant_id: item.tenant_id.clone(),
        }
    }
}

pub(super) enum BatchedMetrics {
    Aggregated(MetricSet),
    Unaggregated(Vec<Metric>),
}

impl BatchedMetrics {
    pub(super) fn into_metrics(self) -> Vec<Metric> {
        match self {
            BatchedMetrics::Aggregated(metrics) => metrics.into_metrics(),
            BatchedMetrics::Unaggregated(metrics) => metrics,
        }
    }

    pub(super) fn insert_update(&mut self, metric: Metric) {
        match self {
            BatchedMetrics::Aggregated(metrics) => metrics.insert_update(metric),
            BatchedMetrics::Unaggregated(metrics) => metrics.push(metric),
        }
    }

    pub(super) fn len(&self) -> usize {
        match self {
            BatchedMetrics::Aggregated(metrics) => metrics.len(),
            BatchedMetrics::Unaggregated(metrics) => metrics.len(),
        }
    }
}

pub(super) struct EventCollection {
    pub(super) finalizers: EventFinalizers,
    pub(super) events: BatchedMetrics,
    pub(super) events_byte_size: usize,
    pub(super) events_json_byte_size: GroupedCountByteSize,
}

impl EventCollection {
    /// Creates a new event collection that will either aggregate the incremental metrics
    /// or store all the metrics, depending on the value of the `aggregate` parameter.
    fn new(aggregate: bool) -> Self {
        Self {
            finalizers: EventFinalizers::default(),
            events: if aggregate {
                BatchedMetrics::Aggregated(MetricSet::default())
            } else {
                BatchedMetrics::Unaggregated(Vec::default())
            },
            events_byte_size: Default::default(),
            events_json_byte_size: telemetry().create_request_count_byte_size(),
        }
    }

    const fn is_aggregated(&self) -> bool {
        matches!(self.events, BatchedMetrics::Aggregated(_))
    }
}

impl BatchData<RemoteWriteMetric> for EventCollection {
    type Batch = Self;

    fn len(&self) -> usize {
        self.events.len()
    }

    fn take_batch(&mut self) -> Self::Batch {
        let mut new = Self::new(self.is_aggregated());
        std::mem::swap(self, &mut new);
        new
    }

    fn push_item(&mut self, mut item: RemoteWriteMetric) {
        self.finalizers
            .merge(item.metric.metadata_mut().take_finalizers());
        self.events_byte_size += item.size_of();
        self.events_json_byte_size
            .add_event(&item.metric, item.estimated_json_encoded_size_of());
        self.events.insert_update(item.metric);
    }
}

// Prometheus treats empty label values as equivalent to absent labels and
// rejects them in Remote Write requests. Stripping them prior to
// normalization ensures metrics with empty tags and metrics without those
// tags map to the same time series.
//
// Uses `as_single()` to stay consistent with `TimeSeries::make_labels` (`into_iter_single`).
pub(super) fn strip_empty_tags(mut metric: Metric) -> Metric {
    if let Some(tags) = metric.tags_mut() {
        tags.retain(|key, values| {
            !key.is_empty() && values.as_single().is_some_and(|v| !v.is_empty())
        });
        if tags.is_empty() {
            metric.remove_tags();
        }
    }
    metric
}

pub(super) struct RemoteWriteSink<S> {
    pub(super) tenant_id: Option<ConfinedTemplate>,
    pub(super) batch_settings: BatcherSettings,
    pub(super) aggregate: bool,
    pub(super) compression: super::Compression,
    pub(super) default_namespace: Option<String>,
    pub(super) buckets: Vec<f64>,
    pub(super) quantiles: Vec<f64>,
    pub(super) expire_metrics_secs: Option<f64>,
    pub(super) service: S,
}

impl<S> RemoteWriteSink<S>
where
    S: Service<RemoteWriteRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = RemoteWriteRequestBuilder {
            compression: self.compression,
            encoder: RemoteWriteEncoder {
                default_namespace: self.default_namespace.clone(),
                buckets: self.buckets.clone(),
                quantiles: self.quantiles.clone(),
            },
        };

        let batch_settings = self.batch_settings;
        let tenant_id = self.tenant_id.clone();
        let service = self.service;
        let expire_metrics_secs = self.expire_metrics_secs;

        input
            .filter_map(|event| future::ready(event.try_into_metric()))
            .map(strip_empty_tags)
            .normalized_with_ttl::<PrometheusMetricNormalize>(expire_metrics_secs)
            .filter_map(move |event| {
                future::ready(make_remote_write_event(tenant_id.as_ref(), event))
            })
            .batched_partitioned(
                PrometheusTenantIdPartitioner,
                batch_settings.timeout,
                |_| {
                    batch_settings
                        .as_reducer_config(ByteSizeOfItemSize, EventCollection::new(self.aggregate))
                },
            )
            .request_builder(default_request_builder_concurrency_limit(), request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build Remote Write request: {e:?}.");
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(service)
            .run()
            .await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for RemoteWriteSink<S>
where
    S: Service<RemoteWriteRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

fn make_remote_write_event(
    tenant_id: Option<&ConfinedTemplate>,
    metric: Metric,
) -> Option<RemoteWriteMetric> {
    let tenant_id = match tenant_id {
        None => None,
        Some(template) => match template.render_string(&metric) {
            Ok(s) => Some(s),
            Err(error) => {
                let confined = matches!(
                    error,
                    crate::template::TemplateRenderingError::Confined { .. }
                );
                emit(TemplateRenderingError {
                    error,
                    field: Some("tenant_id"),
                    drop_event: confined,
                });
                if confined {
                    return None; // Confined — intentional security drop
                }
                None
            }
        },
    };

    Some(RemoteWriteMetric { metric, tenant_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{MetricKind, MetricValue};
    use vector_lib::event::metric::TagValue;

    #[test]
    fn strip_empty_tags_removes_empty_keys_and_values() {
        let metric = Metric::new(
            "requests_total",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(metric_tags!(
            "host" => "host01",
            "timezone" => "",
            "" => "value",
            "bare" => TagValue::Bare,
        )));

        let stripped = strip_empty_tags(metric);
        assert_eq!(stripped.tags(), Some(&metric_tags!("host" => "host01")));
    }

    #[test]
    fn strip_empty_tags_clears_tags_when_all_empty() {
        let metric = Metric::new(
            "requests_total",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.0 },
        )
        .with_tags(Some(metric_tags!(
            "timezone" => "",
            "" => "value",
        )));

        let stripped = strip_empty_tags(metric);
        assert_eq!(stripped.tags(), None);
    }
}
