use crate::encoding::ProtobufSerializer;
use bytes::BytesMut;
use opentelemetry_proto::logs::log_event_to_export_request;
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
use vrl::{event_path, protobuf::encode::Options};

/// Config used to build an `OtlpSerializer`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct OtlpSerializerConfig {
    // No configuration options needed - OTLP serialization is opinionated
}

impl OtlpSerializerConfig {
    /// Build the `OtlpSerializer` from this configuration.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "The codec API error documentation needs a separate audit."
    )]
    pub fn build(&self) -> Result<OtlpSerializer, crate::encoding::BuildError> {
        OtlpSerializer::new()
    }

    /// The data type of events that are accepted by `OtlpSerializer`.
    #[must_use]
    pub fn input_type(&self) -> DataType {
        DataType::all_bits()
    }

    /// The schema required by the serializer.
    #[must_use]
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
/// This serializer converts Vector's internal event representation to the appropriate OTLP message type.
/// Events that already have the OTLP structure are encoded as they are, based on the top-level field:
/// - `resourceLogs` → `ExportLogsServiceRequest`
/// - `resourceMetrics` → `ExportMetricsServiceRequest`
/// - `resourceSpans` → `ExportTraceServiceRequest`
///
/// The first field in this list that the event has is used, and all other event fields are
/// not encoded.
///
/// Native Vector logs and metrics are converted to `ExportLogsServiceRequest` and
/// `ExportMetricsServiceRequest`. The conversions are the inverse of what the `opentelemetry`
/// source does when decoding, ensuring round-trip compatibility.
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
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "The codec API error documentation needs a separate audit."
    )]
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
        match event {
            Event::Log(log) => {
                if log.contains(event_path!(RESOURCE_LOGS_JSON_FIELD)) {
                    self.logs_descriptor.encode(Event::Log(log), buffer)
                } else if log.contains(event_path!(RESOURCE_METRICS_JSON_FIELD)) {
                    // Currently the OTLP metrics are Vector logs (not metrics).
                    self.metrics_descriptor.encode(Event::Log(log), buffer)
                } else if log.contains(event_path!(RESOURCE_SPANS_JSON_FIELD)) {
                    // OTLP-structured traces can be log events, for example when read as JSON.
                    self.traces_descriptor.encode(Event::Log(log), buffer)
                } else {
                    let request = log_event_to_export_request(log);
                    request.encode(buffer).map_err(Into::into)
                }
            }
            Event::Trace(trace) => {
                if trace.contains(event_path!(RESOURCE_SPANS_JSON_FIELD)) {
                    self.traces_descriptor.encode(Event::Trace(trace), buffer)
                } else {
                    Err(format!(
                        "Trace event does not contain OTLP top-level field ({RESOURCE_SPANS_JSON_FIELD})",
                    )
                        .into())
                }
            }
            Event::Metric(metric) => {
                let request = metric_event_to_export_request(metric)?;
                request.encode(buffer).map_err(Into::into)
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
            .flat_map(opentelemetry_proto::proto::metrics::v1::ResourceMetrics::into_event_iter)
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
    fn native_log_round_trips_through_otlp_source_decoding() {
        use opentelemetry_proto::proto::collector::logs::v1::ExportLogsServiceRequest;
        use vector_core::{config::LogNamespace, event::LogEvent};

        let mut log = LogEvent::from("disk full");
        log.insert(event_path!("host"), "web-1");
        log.insert(event_path!("timestamp"), Utc.timestamp_nanos(1_000_000_000));

        let mut buffer = BytesMut::new();
        OtlpSerializer::new()
            .unwrap()
            .encode(Event::Log(log), &mut buffer)
            .expect("native log must be converted, not rejected");

        let request = ExportLogsServiceRequest::decode(buffer.freeze()).unwrap();
        let mut events: Vec<Event> = request
            .resource_logs
            .into_iter()
            .flat_map(|logs| logs.into_event_iter(LogNamespace::Legacy))
            .collect();
        assert_eq!(events.len(), 1);
        let decoded = events.remove(0).into_log();
        assert_eq!(decoded["message"], "disk full".into());
        assert_eq!(decoded["attributes.host"], "web-1".into());
        assert_eq!(
            decoded["timestamp"],
            Utc.timestamp_nanos(1_000_000_000).into()
        );
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
