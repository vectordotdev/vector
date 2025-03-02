#[cfg(test)]
mod tests;

use std::{
    str::FromStr,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, stream, FutureExt, SinkExt};
use futures_util::future;
use http::Request;
use hyper::Body;
use tower::Service;
use vector_lib::{
    codecs::JsonSerializerConfig,
    configurable::configurable_component,
    opentelemetry::proto::{
        collector::metrics::v1::ExportMetricsServiceRequest,
        common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue},
        metrics::v1::{
            metric, number_data_point, AggregationTemporality, Gauge, Histogram,
            HistogramDataPoint, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, Sum,
        },
        resource::v1::Resource,
    },
    sink::VectorSink,
    tls::TlsEnableableConfig,
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};

use crate::{
    codecs::Transformer,
    config::{AcknowledgementsConfig, Input, SinkConfig, SinkContext},
    event::{
        metric::{Metric as VectorMetric, MetricTags, MetricValue},
        Event,
    },
    http::HttpClient,
    sinks::util::{
        batch::BatchConfig,
        buffer::metrics::{MetricNormalize, MetricNormalizer, MetricSet, MetricsBuffer},
        http::HttpStatusRetryLogic,
        Compression, EncodedEvent, PartitionBuffer, PartitionInnerBuffer, SinkBatchSettings,
        TowerRequestConfig,
    },
};

use super::util::{service::TowerRequestConfigDefaults, UriSerde};

#[derive(Clone, Copy, Debug, Default)]
pub struct OpentelemetryMetricsDefaultBatchSettings;

impl SinkBatchSettings for OpentelemetryMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(20);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Clone, Copy, Debug)]
pub struct OpentelemetryMetricsTowerRequestConfigDefaults;

impl TowerRequestConfigDefaults for OpentelemetryMetricsTowerRequestConfigDefaults {
    const RATE_LIMIT_NUM: u64 = 150;
}

/// Configuration for the `opentelemetry_metrics` sink.
#[configurable_component(sink(
    "opentelemetry_metrics",
    "Publish metric events to an OpenTelemetry collector."
))]
#[derive(Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct OpentelemetryMetricsSinkConfig {
    /// The endpoint to send OpenTelemetry metrics to.
    ///
    /// This should be a full URL, including the protocol (e.g. `https://`).
    #[configurable(metadata(docs::examples = "http://localhost:4317"))]
    pub endpoint: String,

    /// The endpoint to send healthcheck requests to.
    ///
    /// This should be a full URL, including the protocol (e.g. `https://`).
    #[configurable(metadata(docs::examples = "http://localhost:13133"))]
    pub healthcheck_endpoint: String,

    /// The default namespace to use for metrics that do not have one.
    ///
    /// Metrics with the same name can only be differentiated by their namespace.
    #[configurable(metadata(docs::examples = "myservice"))]
    pub default_namespace: Option<String>,

    /// The aggregation temporality to use for metrics.
    ///
    /// This determines how metrics are aggregated over time.
    #[serde(default)]
    pub aggregation_temporality: AggregationTemporalityConfig,

    #[configurable(derived)]
    #[serde(default)]
    pub compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    pub batch: BatchConfig<OpentelemetryMetricsDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    pub request: TowerRequestConfig,

    #[configurable(derived)]
    pub tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default)]
    pub encoding: OpentelemetryMetricsEncodingConfig,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

/// The aggregation temporality to use for metrics.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AggregationTemporalityConfig {
    /// Delta temporality means that metrics are reported as changes since the last report.
    Delta,
    /// Cumulative temporality means that metrics are reported as cumulative changes since a fixed start time.
    Cumulative,
}

impl Default for AggregationTemporalityConfig {
    fn default() -> Self {
        Self::Cumulative
    }
}

/// Encoding configuration for OpenTelemetry Metrics.
#[configurable_component]
#[derive(Clone, Debug)]
#[configurable(description = "Configures how events are encoded into raw bytes.")]
pub struct OpentelemetryMetricsEncodingConfig {
    #[serde(flatten)]
    encoding: JsonSerializerConfig,

    #[serde(flatten)]
    transformer: Transformer,
}

impl Default for OpentelemetryMetricsEncodingConfig {
    fn default() -> Self {
        Self {
            encoding: JsonSerializerConfig::default(),
            transformer: Transformer::default(),
        }
    }
}

impl_generate_config_from_default!(OpentelemetryMetricsSinkConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "opentelemetry_metrics")]
impl SinkConfig for OpentelemetryMetricsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = HttpClient::new(None, cx.proxy())?;
        let uri = UriSerde::from_str(&self.healthcheck_endpoint)
            .map_err(|e| crate::Error::from(format!("Invalid healthcheck endpoint: {}", e)))?;

        let healthcheck = healthcheck(uri, client.clone()).boxed();
        let sink = OpentelemetryMetricsSvc::new(self.clone(), client)?;
        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::metric()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

/// Healthcheck for the `opentelemetry_metrics` sink.
///
/// Reference https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/extension/healthcheckextension/README.md
async fn healthcheck(uri: UriSerde, client: HttpClient) -> crate::Result<()> {
    let uri = uri.with_default_parts();
    let request = Request::head(&uri.uri).body(Body::empty()).unwrap();

    let response = client.send(request).await?;

    match response.status() {
        http::StatusCode::OK | http::StatusCode::ACCEPTED | http::StatusCode::NO_CONTENT => Ok(()),
        status => Err(crate::sinks::HealthcheckError::UnexpectedStatus { status }.into()),
    }
}

#[derive(Default)]
struct OpentelemetryMetricNormalize;

impl MetricNormalize for OpentelemetryMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: VectorMetric) -> Option<VectorMetric> {
        match metric.value() {
            MetricValue::Gauge { .. } => state.make_absolute(metric),
            _ => state.make_incremental(metric),
        }
    }
}

fn tags_to_attributes(tags: &MetricTags) -> Vec<KeyValue> {
    tags.iter_single()
        .map(|(k, v)| KeyValue {
            key: k.to_string(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue(v.to_string())),
            }),
        })
        .collect()
}

#[derive(Clone)]
pub struct OpentelemetryMetricsSvc {
    config: OpentelemetryMetricsSinkConfig,
    client: HttpClient,
}

impl OpentelemetryMetricsSvc {
    pub fn new(
        config: OpentelemetryMetricsSinkConfig,
        client: HttpClient,
    ) -> crate::Result<VectorSink> {
        let default_namespace = config
            .default_namespace
            .clone()
            .unwrap_or_else(|| "vector".into());
        let batch = config.batch.into_batch_settings()?;
        let request_settings = config.request.into_settings();

        let service = OpentelemetryMetricsSvc { config, client };
        let buffer = PartitionBuffer::new(MetricsBuffer::new(batch.size));
        let mut normalizer = MetricNormalizer::<OpentelemetryMetricNormalize>::default();

        let sink = request_settings
            .partition_sink(
                HttpStatusRetryLogic::new(|resp: &http::Response<hyper::Body>| resp.status()),
                service,
                buffer,
                batch.timeout,
            )
            .sink_map_err(
                |error| error!(message = "Fatal OpentelemetryMetrics sink error.", %error),
            )
            .with_flat_map(move |event: Event| {
                stream::iter({
                    let byte_size = event.allocated_bytes();
                    let json_byte_size = event.estimated_json_encoded_size_of();
                    normalizer.normalize(event.into_metric()).map(|mut metric| {
                        let namespace = metric
                            .take_namespace()
                            .unwrap_or_else(|| default_namespace.clone());
                        Ok(EncodedEvent::new(
                            PartitionInnerBuffer::new(metric, namespace),
                            byte_size,
                            json_byte_size,
                        ))
                    })
                })
            });

        #[allow(deprecated)]
        Ok(VectorSink::from_event_sink(sink))
    }

    fn encode_events(&mut self, events: Vec<VectorMetric>) -> Vec<Metric> {
        events
            .into_iter()
            .filter_map(|event| {
                let metric_name = event.name().to_string();
                let timestamp = event
                    .timestamp()
                    .map(|x| x.timestamp_nanos_opt().unwrap_or(0) as u64);
                let attributes = event.tags().map(tags_to_attributes).unwrap_or_default();

                // Convert Vector metrics to OpenTelemetry metrics
                match event.value() {
                    MetricValue::Counter { value } => {
                        let data_point = NumberDataPoint {
                            attributes,
                            time_unix_nano: timestamp.unwrap_or_else(|| {
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_nanos() as u64
                            }),
                            start_time_unix_nano: 0, // We don't have start time in Vector metrics
                            value: Some(number_data_point::Value::AsDouble(*value)),
                            exemplars: Vec::new(),
                            flags: 0,
                        };

                        let aggregation_temporality = match self.config.aggregation_temporality {
                            AggregationTemporalityConfig::Delta => AggregationTemporality::Delta,
                            AggregationTemporalityConfig::Cumulative => {
                                AggregationTemporality::Cumulative
                            }
                        };

                        Some(Metric {
                            name: metric_name,
                            description: String::new(),
                            unit: String::new(),
                            data: Some(metric::Data::Sum(Sum {
                                data_points: vec![data_point],
                                aggregation_temporality: aggregation_temporality as i32,
                                is_monotonic: true,
                            })),
                        })
                    }
                    MetricValue::Gauge { value } => {
                        let data_point = NumberDataPoint {
                            attributes,
                            time_unix_nano: timestamp.unwrap_or_else(|| {
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_nanos() as u64
                            }),
                            start_time_unix_nano: 0, // Not needed for gauges
                            value: Some(number_data_point::Value::AsDouble(*value)),
                            exemplars: Vec::new(),
                            flags: 0,
                        };

                        Some(Metric {
                            name: metric_name,
                            description: String::new(),
                            unit: String::new(),
                            data: Some(metric::Data::Gauge(Gauge {
                                data_points: vec![data_point],
                            })),
                        })
                    }
                    MetricValue::Distribution {
                        samples,
                        statistic: _,
                    } => {
                        // Convert to histogram
                        let mut sum = 0.0;
                        let mut count = 0;
                        let mut bucket_counts = Vec::new();
                        let mut explicit_bounds = Vec::new();

                        // Simple conversion - this could be improved with better bucket boundaries
                        for sample in samples {
                            sum += sample.value * sample.rate as f64;
                            count += sample.rate;

                            // Add a bucket for each sample
                            explicit_bounds.push(sample.value);
                            bucket_counts.push(sample.rate as u64);
                        }

                        // Add the final bucket (infinity)
                        bucket_counts.push(0);

                        let data_point = HistogramDataPoint {
                            attributes,
                            time_unix_nano: timestamp.unwrap_or_else(|| {
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_nanos() as u64
                            }),
                            start_time_unix_nano: 0,
                            count: count as u64,
                            sum: Some(sum),
                            bucket_counts,
                            explicit_bounds,
                            exemplars: Vec::new(),
                            flags: 0,
                            min: None,
                            max: None,
                        };

                        let aggregation_temporality = match self.config.aggregation_temporality {
                            AggregationTemporalityConfig::Delta => AggregationTemporality::Delta,
                            AggregationTemporalityConfig::Cumulative => {
                                AggregationTemporality::Cumulative
                            }
                        };

                        Some(Metric {
                            name: metric_name,
                            description: String::new(),
                            unit: String::new(),
                            data: Some(metric::Data::Histogram(Histogram {
                                data_points: vec![data_point],
                                aggregation_temporality: aggregation_temporality as i32,
                            })),
                        })
                    }
                    MetricValue::Set { values } => {
                        // Convert to a sum with the count of unique values
                        let data_point = NumberDataPoint {
                            attributes,
                            time_unix_nano: timestamp.unwrap_or_else(|| {
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_nanos() as u64
                            }),
                            start_time_unix_nano: 0,
                            value: Some(number_data_point::Value::AsDouble(values.len() as f64)),
                            exemplars: Vec::new(),
                            flags: 0,
                        };

                        let aggregation_temporality = match self.config.aggregation_temporality {
                            AggregationTemporalityConfig::Delta => AggregationTemporality::Delta,
                            AggregationTemporalityConfig::Cumulative => {
                                AggregationTemporality::Cumulative
                            }
                        };

                        Some(Metric {
                            name: metric_name,
                            description: String::new(),
                            unit: String::new(),
                            data: Some(metric::Data::Sum(Sum {
                                data_points: vec![data_point],
                                aggregation_temporality: aggregation_temporality as i32,
                                is_monotonic: false,
                            })),
                        })
                    }
                    _ => None,
                }
            })
            .collect()
    }
}

impl Service<PartitionInnerBuffer<Vec<VectorMetric>, String>> for OpentelemetryMetricsSvc {
    type Response = http::Response<hyper::Body>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, items: PartitionInnerBuffer<Vec<VectorMetric>, String>) -> Self::Future {
        let (items, namespace) = items.into_parts();
        let metrics = self.encode_events(items);
        if metrics.is_empty() {
            return Box::pin(future::ok(
                http::Response::builder()
                    .status(http::StatusCode::OK)
                    .body(hyper::Body::empty())
                    .unwrap(),
            ));
        }

        let client = self.client.clone();
        let namespace_clone = namespace.clone();
        let endpoint = self.config.endpoint.clone();

        Box::pin(async move {
            // Create the ResourceMetrics structure
            let resource_metrics = ResourceMetrics {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue(namespace)),
                        }),
                    }],
                    dropped_attributes_count: 0,
                }),
                scope_metrics: vec![ScopeMetrics {
                    scope: Some(InstrumentationScope {
                        name: "vector".to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        attributes: Vec::new(),
                        dropped_attributes_count: 0,
                    }),
                    metrics: metrics.clone(),
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            };

            // Create the request body
            let _metrics_data = ExportMetricsServiceRequest {
                resource_metrics: vec![resource_metrics],
            };

            // We need to convert the metrics to JSON manually since the proto types don't implement Serialize
            let json_body = serde_json::json!({
                "resourceMetrics": [{
                    "resource": {
                        "attributes": [{
                            "key": "service.name",
                            "value": {
                                "stringValue": namespace_clone
                            }
                        }]
                    },
                    "scopeMetrics": [{
                        "scope": {
                            "name": "vector",
                            "version": env!("CARGO_PKG_VERSION")
                        },
                        "metrics": metrics.iter().map(|m| {
                            let mut metric_json = serde_json::json!({
                                "name": m.name,
                                "description": m.description,
                                "unit": m.unit
                            });

                            if let Some(data) = &m.data {
                                match data {
                                    metric::Data::Gauge(gauge) => {
                                        metric_json["gauge"] = serde_json::json!({
                                            "dataPoints": gauge.data_points.iter().map(|dp| {
                                                serde_json::json!({
                                                    "timeUnixNano": dp.time_unix_nano,
                                                    "asDouble": match dp.value {
                                                        Some(number_data_point::Value::AsDouble(v)) => v,
                                                        _ => 0.0
                                                    }
                                                })
                                            }).collect::<Vec<_>>()
                                        });
                                    },
                                    metric::Data::Sum(sum) => {
                                        metric_json["sum"] = serde_json::json!({
                                            "dataPoints": sum.data_points.iter().map(|dp| {
                                                serde_json::json!({
                                                    "timeUnixNano": dp.time_unix_nano,
                                                    "asDouble": match dp.value {
                                                        Some(number_data_point::Value::AsDouble(v)) => v,
                                                        _ => 0.0
                                                    }
                                                })
                                            }).collect::<Vec<_>>(),
                                            "aggregationTemporality": sum.aggregation_temporality,
                                            "isMonotonic": sum.is_monotonic
                                        });
                                    },
                                    metric::Data::Histogram(histogram) => {
                                        metric_json["histogram"] = serde_json::json!({
                                            "dataPoints": histogram.data_points.iter().map(|dp| {
                                                serde_json::json!({
                                                    "timeUnixNano": dp.time_unix_nano,
                                                    "count": dp.count,
                                                    "sum": dp.sum,
                                                    "bucketCounts": dp.bucket_counts,
                                                    "explicitBounds": dp.explicit_bounds
                                                })
                                            }).collect::<Vec<_>>(),
                                            "aggregationTemporality": histogram.aggregation_temporality
                                        });
                                    },
                                    _ => {}
                                }
                            }

                            metric_json
                        }).collect::<Vec<_>>()
                    }]
                }]
            });

            let body = serde_json::to_vec(&json_body)
                .map_err(|e| crate::Error::from(format!("JSON serialization error: {}", e)))?;

            let uri = UriSerde::from_str(&endpoint)
                .map_err(|e| crate::Error::from(format!("Invalid endpoint: {}", e)))?;

            let request = Request::post(uri.with_default_parts().uri)
                .header("Content-Type", "application/json")
                .body(hyper::Body::from(body))
                .map_err(|e| crate::Error::from(format!("Error building request: {}", e)))?;

            // Send the request
            client.send(request).await.map_err(Into::into)
        })
    }
}
