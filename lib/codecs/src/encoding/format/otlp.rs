use crate::encoding::ProtobufSerializer;
use bytes::BytesMut;
use opentelemetry_proto::metrics::metric_event_to_export_request;
use opentelemetry_proto::proto::{
    DESCRIPTOR_BYTES, LOGS_REQUEST_MESSAGE_TYPE, METRICS_REQUEST_MESSAGE_TYPE,
    RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
    TRACES_REQUEST_MESSAGE_TYPE,
};
use prost::Message;
use tokio_util::codec::Encoder;
use vector_config_macros::configurable_component;
use vector_core::{config::DataType, event::Event, schema};
use vrl::protobuf::encode::Options;

/// Config used to build an `OtlpSerializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct OtlpSerializerConfig {
    // No configuration options needed - OTLP serialization is opinionated
}

impl OtlpSerializerConfig {
    /// Build the `OtlpSerializer` from this configuration.
    pub fn build(&self) -> Result<OtlpSerializer, crate::encoding::BuildError> {
        OtlpSerializer::new()
    }

    /// The data type of events that are accepted by `OtlpSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log | DataType::Trace | DataType::Metric
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Serializer that converts an `Event` to bytes using the OTLP (OpenTelemetry Protocol) protobuf format.
///
/// This serializer encodes events using the OTLP protobuf specification, which is the recommended
/// encoding format for OpenTelemetry data. The output is suitable for sending to OTLP-compatible
/// endpoints with `content-type: application/x-protobuf`.
///
/// # Implementation approach
///
/// This serializer converts Vector's internal event representation to the appropriate OTLP message type
/// based on the top-level field in the event:
/// - `resourceLogs` → `ExportLogsServiceRequest`
/// - `resourceMetrics` → `ExportMetricsServiceRequest`
/// - `resourceSpans` → `ExportTraceServiceRequest`
///
/// The implementation is the inverse of what the `opentelemetry` source does when decoding,
/// ensuring round-trip compatibility.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields will be used once encoding is implemented
pub struct OtlpSerializer {
    logs_descriptor: ProtobufSerializer,
    metrics_descriptor: ProtobufSerializer,
    traces_descriptor: ProtobufSerializer,
    options: Options,
}

impl OtlpSerializer {
    /// Creates a new OTLP serializer with the appropriate message descriptors.
    pub fn new() -> vector_common::Result<Self> {
        let options = Options {
            use_json_names: true,
            allow_lossy_string_coercion: true,
        };

        let logs_descriptor = ProtobufSerializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            LOGS_REQUEST_MESSAGE_TYPE,
            &options,
        )?;

        let metrics_descriptor = ProtobufSerializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            METRICS_REQUEST_MESSAGE_TYPE,
            &options,
        )?;

        let traces_descriptor = ProtobufSerializer::new_from_bytes(
            DESCRIPTOR_BYTES,
            TRACES_REQUEST_MESSAGE_TYPE,
            &options,
        )?;

        Ok(Self {
            logs_descriptor,
            metrics_descriptor,
            traces_descriptor,
            options,
        })
    }
}

impl Encoder<Event> for OtlpSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        // Determine which descriptor to use based on top-level OTLP fields
        // This handles events that were decoded with use_otlp_decoding enabled
        // The deserializer uses use_json_names: true, so fields are in camelCase
        match &event {
            Event::Log(log) => {
                if log.contains(RESOURCE_LOGS_JSON_FIELD) {
                    self.logs_descriptor.encode(event, buffer)
                } else if log.contains(RESOURCE_METRICS_JSON_FIELD) {
                    // Currently the OTLP metrics are Vector logs (not metrics).
                    self.metrics_descriptor.encode(event, buffer)
                } else {
                    Err(format!(
                        "Log event does not contain OTLP top-level fields ({RESOURCE_LOGS_JSON_FIELD} or {RESOURCE_METRICS_JSON_FIELD})",
                    )
                        .into())
                }
            }
            Event::Trace(trace) => {
                if trace.contains(RESOURCE_SPANS_JSON_FIELD) {
                    self.traces_descriptor.encode(event, buffer)
                } else {
                    Err(format!(
                        "Trace event does not contain OTLP top-level field ({RESOURCE_SPANS_JSON_FIELD})",
                    )
                        .into())
                }
            }
            Event::Metric(metric) => {
                let request = metric_event_to_export_request(metric.clone())?;
                buffer.extend_from_slice(&request.encode_to_vec());
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use opentelemetry_proto::proto::collector::metrics::v1::ExportMetricsServiceRequest;
    use vector_core::event::{Metric, MetricKind, MetricTags, MetricValue, metric::Bucket};

    // `into_event_iter` always wraps attributes in `Some(MetricTags)` (via `build_metric_tags`),
    // even when there are none, so a tag-less input compares unequal to its round-tripped output
    // unless we give it the same empty-but-present tag set up front.
    fn with_empty_tags(metric: Metric) -> Metric {
        metric.with_tags(Some(MetricTags::default()))
    }

    fn round_trip_metric(metric: Metric) -> Metric {
        let mut serializer = OtlpSerializer::new().unwrap();
        let mut buffer = BytesMut::new();
        serializer
            .encode(Event::Metric(metric), &mut buffer)
            .expect("encode should succeed");

        let request =
            ExportMetricsServiceRequest::decode(buffer.freeze()).expect("decode should succeed");
        let mut events: Vec<Event> = request
            .resource_metrics
            .into_iter()
            .flat_map(|rm| rm.into_event_iter())
            .collect();

        assert_eq!(events.len(), 1);
        match events.remove(0) {
            Event::Metric(metric) => metric,
            other => panic!("expected a metric event, got {other:?}"),
        }
    }

    #[test]
    fn round_trip_counter() {
        let metric = with_empty_tags(
            Metric::new(
                "requests",
                MetricKind::Incremental,
                MetricValue::Counter { value: 42.0 },
            )
            .with_timestamp(Some(Utc.timestamp_nanos(1_000_000_000))),
        );

        assert_eq!(metric.clone(), round_trip_metric(metric));
    }

    #[test]
    fn round_trip_gauge() {
        let metric = with_empty_tags(
            Metric::new(
                "cpu_usage",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 12.5 },
            )
            .with_timestamp(Some(Utc.timestamp_nanos(1_000_000_000))),
        );

        assert_eq!(metric.clone(), round_trip_metric(metric));
    }

    #[test]
    fn round_trip_aggregated_histogram() {
        let metric = with_empty_tags(
            Metric::new(
                "latency",
                MetricKind::Absolute,
                MetricValue::AggregatedHistogram {
                    buckets: vec![
                        Bucket {
                            upper_limit: 1.0,
                            count: 1,
                        },
                        Bucket {
                            upper_limit: 2.0,
                            count: 2,
                        },
                        Bucket {
                            upper_limit: f64::INFINITY,
                            count: 3,
                        },
                    ],
                    count: 6,
                    sum: 10.0,
                },
            )
            .with_timestamp(Some(Utc.timestamp_nanos(1_000_000_000))),
        );

        assert_eq!(metric.clone(), round_trip_metric(metric));
    }

    #[test]
    fn round_trip_aggregated_summary() {
        let metric = with_empty_tags(
            Metric::new(
                "response_time",
                MetricKind::Absolute,
                MetricValue::AggregatedSummary {
                    quantiles: vec![
                        vector_core::event::metric::Quantile {
                            quantile: 0.5,
                            value: 10.0,
                        },
                        vector_core::event::metric::Quantile {
                            quantile: 0.99,
                            value: 20.0,
                        },
                    ],
                    count: 100,
                    sum: 1000.0,
                },
            )
            .with_timestamp(Some(Utc.timestamp_nanos(1_000_000_000))),
        );

        assert_eq!(metric.clone(), round_trip_metric(metric));
    }

    #[test]
    fn unsupported_metric_values_return_err() {
        let mut serializer = OtlpSerializer::new().unwrap();
        let mut buffer = BytesMut::new();

        let set_metric = Metric::new(
            "unique_users",
            MetricKind::Incremental,
            MetricValue::Set {
                values: std::iter::once("a".to_string()).collect(),
            },
        );
        assert!(
            serializer
                .encode(Event::Metric(set_metric), &mut buffer)
                .is_err()
        );

        let distribution_metric = Metric::new(
            "latencies",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: Vec::new(),
                statistic: vector_core::event::metric::StatisticKind::Histogram,
            },
        );
        assert!(
            serializer
                .encode(Event::Metric(distribution_metric), &mut buffer)
                .is_err()
        );
    }
}
