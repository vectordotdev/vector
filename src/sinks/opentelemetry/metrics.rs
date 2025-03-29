use crate::event::metric::{Metric as VectorMetric, MetricValue};
use std::task::{Context, Poll};
use vector_config::configurable_component;

use futures::future::{self, BoxFuture};
use http::StatusCode;
use hyper::Body;
use tower::Service;
use tracing::{debug, trace};
use vector_lib::event::EventStatus;

use opentelemetry::metrics::{Meter, MeterProvider};
use opentelemetry::KeyValue;
use opentelemetry_otlp::{MetricExporter, WithExportConfig};
use opentelemetry_sdk::metrics::{SdkMeterProvider, Temporality};

use crate::event::Event;
use crate::sinks::util::PartitionInnerBuffer;
use futures_util::stream::BoxStream;
use vector_lib::sink::StreamSink;

/// The aggregation temporality to use for metrics.
#[configurable_component]
#[derive(Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AggregationTemporality {
    /// Delta temporality means that metrics are reported as changes since the last report.
    Delta,
    /// Cumulative temporality means that metrics are reported as cumulative changes since a fixed start time.
    Cumulative,
}

impl Default for AggregationTemporality {
    fn default() -> Self {
        Self::Cumulative
    }
}

// Add conversion from AggregationTemporality to the OpenTelemetry SDK's Temporality
impl From<AggregationTemporality> for Temporality {
    fn from(temporality: AggregationTemporality) -> Self {
        match temporality {
            AggregationTemporality::Delta => Temporality::Delta,
            AggregationTemporality::Cumulative => Temporality::Cumulative,
        }
    }
}

#[derive(Default)]
pub struct OpentelemetryMetricNormalize;

// Implementation using the OpenTelemetry SDK
pub struct OpentelemetryMetricsSvc {
    meter_provider: SdkMeterProvider,
    meter: Meter,
    namespace: String,
}

impl OpentelemetryMetricsSvc {
    pub fn new(
        namespace: String,
        endpoint: String,
        temporality: AggregationTemporality,
    ) -> crate::Result<Self> {
        // Create the exporter
        let exporter = MetricExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .with_temporality(Temporality::from(temporality))
            .build()
            .map_err(|e| crate::Error::from(format!("Failed to build metrics exporter: {}", e)))?;

        // Create the meter provider with the exporter
        let provider = SdkMeterProvider::builder()
            .with_periodic_exporter(exporter)
            .build();

        let meter = provider.meter("vector");

        Ok(Self {
            meter_provider: provider,
            meter,
            namespace,
        })
    }

    // Convert and record Vector metrics using the OpenTelemetry SDK
    fn convert_and_record_metrics(&self, events: Vec<VectorMetric>) {
        for event in events {
            let metric_name = event.name().to_string();
            let attributes = event
                .tags()
                .map(|tags| {
                    tags.iter_single()
                        .map(|(k, v)| KeyValue::new(k.to_string(), v.to_string()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            // Add the service.name attribute with the namespace
            let mut all_attributes = vec![KeyValue::new("service.name", self.namespace.clone())];
            all_attributes.extend(attributes);

            match event.value() {
                MetricValue::Counter { value } => {
                    let counter = self.meter.f64_counter(metric_name).build();
                    counter.add(*value, &all_attributes);
                }
                MetricValue::Gauge { value } => {
                    // For gauges, we use a counter since observable gauges require callbacks
                    let counter = self
                        .meter
                        .f64_counter(format!("{}_gauge", metric_name))
                        .build();
                    counter.add(*value, &all_attributes);
                }
                MetricValue::Distribution { samples, .. } => {
                    let histogram = self.meter.f64_histogram(metric_name).build();
                    for sample in samples {
                        // Record each sample with its rate
                        for _ in 0..sample.rate {
                            histogram.record(sample.value, &all_attributes);
                        }
                    }
                }
                MetricValue::Set { values } => {
                    // For sets, we record the count of unique values
                    let counter = self
                        .meter
                        .f64_counter(format!("{}_set", metric_name))
                        .build();
                    counter.add(values.len() as f64, &all_attributes);
                }
                _ => {}
            }
        }
    }
}

impl Service<PartitionInnerBuffer<Vec<VectorMetric>, String>> for OpentelemetryMetricsSvc {
    type Response = http::Response<Body>;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, items: PartitionInnerBuffer<Vec<VectorMetric>, String>) -> Self::Future {
        let (metrics, _namespace) = items.into_parts();

        // Convert and record metrics
        self.convert_and_record_metrics(metrics);

        // The SDK handles the export asynchronously, so we just return a success response
        Box::pin(future::ok(
            http::Response::builder()
                .status(StatusCode::OK)
                .body(Body::empty())
                .unwrap(),
        ))
    }
}

impl Drop for OpentelemetryMetricsSvc {
    fn drop(&mut self) {
        // Ensure metrics are exported before shutting down
        if let Err(err) = self.meter_provider.shutdown() {
            error!("Error shutting down meter provider: {:?}", err);
        }
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for OpentelemetryMetricsSvc {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        use futures::StreamExt;

        debug!("OpenTelemetry metrics sink started");

        while let Some(mut event) = input.next().await {
            // Extract finalizers before processing
            let finalizers = event.metadata_mut().take_finalizers();

            // Extract metrics from the event
            if let Event::Metric(metric) = event {
                trace!("Processing metric event: {}", metric.name());
                // Process the metric
                self.convert_and_record_metrics(vec![metric]);
            } else {
                trace!("Ignoring non-metric event");
            }

            // Finalize the event with success status
            finalizers.update_status(EventStatus::Delivered);
        }

        debug!("OpenTelemetry metrics sink stopped");
        Ok(())
    }
}
