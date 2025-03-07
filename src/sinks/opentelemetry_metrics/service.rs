use std::task::{Context, Poll};

use futures::{
    future::{self, BoxFuture},
    stream, SinkExt,
};
use http::{Request, StatusCode};
use hyper::Body;
use tower::Service;
use vector_lib::{
    opentelemetry::proto::{
        collector::metrics::v1::ExportMetricsServiceRequest,
        common::v1::{any_value, AnyValue, InstrumentationScope, KeyValue},
        metrics::v1::{metric, number_data_point, ResourceMetrics, ScopeMetrics},
        resource::v1::Resource,
    },
    sink::VectorSink,
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};

use crate::{
    event::{metric::Metric, metric::MetricValue, Event},
    http::HttpClient,
    sinks::util::{
        buffer::metrics::{MetricNormalize, MetricNormalizer, MetricSet, MetricsBuffer},
        http::HttpStatusRetryLogic,
        EncodedEvent, PartitionBuffer, PartitionInnerBuffer,
    },
};

use super::{config::OpentelemetryMetricsSinkConfig, encoder::encode_metrics};

#[derive(Default)]
pub struct OpentelemetryMetricNormalize;

impl MetricNormalize for OpentelemetryMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        match metric.value() {
            MetricValue::Gauge { .. } => state.make_absolute(metric),
            _ => state.make_incremental(metric),
        }
    }
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
                HttpStatusRetryLogic::new(|resp: &http::Response<Body>| resp.status()),
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
}

impl Service<PartitionInnerBuffer<Vec<Metric>, String>> for OpentelemetryMetricsSvc {
    type Response = http::Response<Body>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, items: PartitionInnerBuffer<Vec<Metric>, String>) -> Self::Future {
        let (items, namespace) = items.into_parts();
        let metrics = encode_metrics(items, self.config.aggregation_temporality);
        if metrics.is_empty() {
            return Box::pin(future::ok(
                http::Response::builder()
                    .status(StatusCode::OK)
                    .body(Body::empty())
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

            let request = Request::post(&endpoint)
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .map_err(|e| crate::Error::from(format!("Error building request: {}", e)))?;

            // Send the request
            client.send(request).await.map_err(Into::into)
        })
    }
}
