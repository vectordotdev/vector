use crate::encoding::ProtobufSerializer;
use crate::internal_events::OtlpTraceConversionIssue;
use bytes::BytesMut;
use opentelemetry_proto::metrics::metric_event_to_export_request;
use opentelemetry_proto::proto::{
    DESCRIPTOR_BYTES, LOGS_REQUEST_MESSAGE_TYPE, METRICS_REQUEST_MESSAGE_TYPE,
    RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, RESOURCE_SPANS_JSON_FIELD,
    TRACES_REQUEST_MESSAGE_TYPE, collector::trace::v1::ExportTraceServiceRequest,
};
use opentelemetry_proto::typed_trace::legacy_to_typed;
use prost::Message;
use tokio_util::codec::Encoder;
use vector_common::internal_event::emit;
use vector_config_macros::configurable_component;
use vector_core::{
    config::DataType,
    event::{
        Event, TraceEvent,
        typed_trace::{TraceConversionIssue, TraceConversionReporter},
    },
    schema,
};
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
                } else {
                    Err(format!(
                        "Log event does not contain OTLP top-level fields ({RESOURCE_LOGS_JSON_FIELD} or {RESOURCE_METRICS_JSON_FIELD})",
                    )
                        .into())
                }
            }
            Event::Trace(trace) => {
                if trace.contains(event_path!(RESOURCE_SPANS_JSON_FIELD)) {
                    self.traces_descriptor.encode(Event::Trace(trace), buffer)
                } else {
                    native_trace_to_export_request(trace)?
                        .encode(buffer)
                        .map_err(Into::into)
                }
            }
            Event::Metric(metric) => {
                let request = metric_event_to_export_request(metric)?;
                request.encode(buffer).map_err(Into::into)
            }
        }
    }
}

/// Convert a native trace event from the `opentelemetry` source (one flattened span per event)
/// into an OTLP export request, through the typed trace model (RFC 25329).
///
/// Fails for traces in other layouts, such as Datadog traces, and for traces where every span
/// is rejected, so that nothing is sent for them.
fn native_trace_to_export_request(
    trace: TraceEvent,
) -> vector_common::Result<ExportTraceServiceRequest> {
    let mut reporter = EmitTraceConversionIssue;
    let events = legacy_to_typed(trace, &mut reporter)
        .map_err(|failed| -> vector_common::Error { Box::new(failed.error()) })?;
    if events.is_empty() {
        return Err("trace event has no spans that can be encoded as OTLP".into());
    }
    // The finalizers belong to the input event. Sinks take them before encoding, so this
    // set is empty and dropping it acknowledges nothing.
    let (request, _finalizers) = ExportTraceServiceRequest::from_events(events, &mut reporter);
    Ok(request)
}

struct EmitTraceConversionIssue;

impl TraceConversionReporter for EmitTraceConversionIssue {
    fn report(&mut self, issue: TraceConversionIssue<'_>) {
        emit(OtlpTraceConversionIssue { issue });
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

    mod native_traces {
        use opentelemetry_proto::proto::{
            common::v1::{AnyValue, KeyValue, any_value::Value as PBValue},
            resource::v1::Resource,
            trace::v1::{ResourceSpans, ScopeSpans, Span},
        };
        use vector_core::event::TraceLayout;

        use super::*;

        const TRACE_ID: [u8; 16] = [1; 16];
        const SPAN_ID: [u8; 8] = [2; 8];

        /// Decode one span the way the `opentelemetry` source does without
        /// `use_otlp_decoding`, giving a flattened native trace event.
        fn flattened_trace(trace_id: &[u8]) -> Event {
            let resource_spans = ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".into(),
                        value: Some(AnyValue {
                            value: Some(PBValue::StringValue("checkout".into())),
                        }),
                    }],
                    ..Default::default()
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![Span {
                        trace_id: trace_id.to_vec(),
                        span_id: SPAN_ID.to_vec(),
                        name: "GET /cart".into(),
                        kind: 2,
                        start_time_unix_nano: 1_000,
                        end_time_unix_nano: 2_000,
                        ..Default::default()
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            };
            let mut events: Vec<Event> = resource_spans.into_event_iter().collect();
            assert_eq!(events.len(), 1);
            events.remove(0)
        }

        #[test]
        fn flattened_trace_is_converted() {
            let mut buffer = BytesMut::new();
            OtlpSerializer::new()
                .unwrap()
                .encode(flattened_trace(&TRACE_ID), &mut buffer)
                .expect("native trace must be converted, not rejected");

            let request = ExportTraceServiceRequest::decode(buffer.freeze()).unwrap();
            assert_eq!(request.resource_spans.len(), 1);
            let resource_spans = &request.resource_spans[0];
            let resource = resource_spans.resource.as_ref().unwrap();
            assert_eq!(resource.attributes[0].key, "service.name");
            let spans = &resource_spans.scope_spans[0].spans;
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].trace_id, TRACE_ID);
            assert_eq!(spans[0].span_id, SPAN_ID);
            assert_eq!(spans[0].name, "GET /cart");
            assert_eq!(spans[0].kind, 2);
            assert_eq!(spans[0].start_time_unix_nano, 1_000);
            assert_eq!(spans[0].end_time_unix_nano, 2_000);
        }

        #[test]
        fn trace_with_only_rejected_spans_is_an_error() {
            let mut buffer = BytesMut::new();
            let result = OtlpSerializer::new()
                .unwrap()
                .encode(flattened_trace(&[0; 16]), &mut buffer);
            assert!(result.is_err());
            assert!(buffer.is_empty());
        }

        #[test]
        fn datadog_trace_is_an_error() {
            let mut trace = TraceEvent::default();
            trace.insert(event_path!("spans"), vrl::value::Value::Array(Vec::new()));
            trace.metadata_mut().set_trace_layout(TraceLayout::Datadog);

            let mut buffer = BytesMut::new();
            let result = OtlpSerializer::new()
                .unwrap()
                .encode(Event::Trace(trace), &mut buffer);
            assert!(result.is_err());
            assert!(buffer.is_empty());
        }
    }
}
