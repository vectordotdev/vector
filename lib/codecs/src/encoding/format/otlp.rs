use crate::encoding::ProtobufSerializer;
use bytes::BytesMut;
use opentelemetry_proto::metrics::metric_event_to_export_request;
#[cfg(not(feature = "typed-trace"))]
use opentelemetry_proto::proto::RESOURCE_SPANS_JSON_FIELD;
use opentelemetry_proto::proto::{
    DESCRIPTOR_BYTES, LOGS_REQUEST_MESSAGE_TYPE, METRICS_REQUEST_MESSAGE_TYPE,
    RESOURCE_LOGS_JSON_FIELD, RESOURCE_METRICS_JSON_FIELD, TRACES_REQUEST_MESSAGE_TYPE,
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
    ///
    /// # Errors
    ///
    /// Returns an error if the bundled OTLP descriptor set cannot be decoded or does not contain
    /// the logs, metrics, or traces export request message types.
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
    #[cfg_attr(feature = "typed-trace", expect(dead_code))]
    traces_descriptor: ProtobufSerializer,
    options: Options,
}

impl OtlpSerializer {
    /// Creates a new OTLP serializer with the appropriate message descriptors.
    ///
    /// # Errors
    ///
    /// Returns an error if the bundled OTLP descriptor set cannot be decoded or does not contain
    /// the logs, metrics, or traces export request message types.
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
            #[cfg(feature = "typed-trace")]
            Event::Trace(trace) => encode_typed_trace(trace, buffer),
            #[cfg(not(feature = "typed-trace"))]
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

/// Encodes a trace through the typed model (RFC 25329).
///
/// Reported conversion issues do not fail the event: accepted spans are still encoded. The
/// error return is reserved for a conversion that cannot interpret the legacy layout.
#[cfg(feature = "typed-trace")]
fn encode_typed_trace(
    trace: vector_core::event::TraceEvent,
    buffer: &mut BytesMut,
) -> vector_common::Result<()> {
    use opentelemetry_proto::{
        proto::collector::trace::v1::ExportTraceServiceRequest, typed_trace::legacy_to_typed,
    };

    let mut reporter = OtlpTraceReporter;
    let events = legacy_to_typed(trace, &mut reporter)?;
    // Sinks take finalizers before encoding, so dropping these releases nothing early.
    let (request, _finalizers) = ExportTraceServiceRequest::from_events(events, &mut reporter);
    request.encode(buffer).map_err(Into::into)
}

/// Reports typed-trace conversion issues while encoding continues.
#[cfg(feature = "typed-trace")]
struct OtlpTraceReporter;

#[cfg(feature = "typed-trace")]
impl vector_core::event::typed_trace::TraceConversionReporter for OtlpTraceReporter {
    fn report(&mut self, issue: vector_core::event::typed_trace::TraceConversionIssue<'_>) {
        use vector_common::internal_event::emit;

        emit(crate::internal_events::OtlpTraceConversionError { issue });
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
                    sum: Some(10.0),
                },
            )
            .with_timestamp(Some(Utc.timestamp_nanos(1_000_000_000))),
        );

        assert_eq!(metric.clone(), round_trip_metric(metric));
    }

    /// OTLP's `sum` is optional, so a histogram reporting none encodes with the field unset and
    /// decodes back to the same thing. Substituting a zero on the way out would manufacture a
    /// measurement that the producer never made.
    #[test]
    fn round_trip_aggregated_histogram_without_sum() {
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
                            upper_limit: f64::INFINITY,
                            count: 2,
                        },
                    ],
                    count: 3,
                    sum: None,
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

/// A rejected span must not fail encoding of its accepted siblings.
#[cfg(all(test, feature = "typed-trace"))]
mod typed_trace_tests {
    use bytes::BytesMut;
    use opentelemetry_proto::proto::collector::trace::v1::ExportTraceServiceRequest;
    use prost::Message;
    use tokio_util::codec::Encoder;
    use vector_core::event::{Event, TraceEvent, TraceLayout};
    use vrl::{event_path, value::ObjectMap, value::Value};

    use super::OtlpSerializer;

    fn raw_id(value: u128, len: usize) -> Value {
        let bytes = value.to_be_bytes();
        Value::Bytes(bytes[16 - len..].to_vec().into())
    }

    fn span(name: &str, trace: u128, span_id: u64) -> Value {
        Value::Object(ObjectMap::from([
            ("traceId".into(), raw_id(trace, 16)),
            ("spanId".into(), raw_id(u128::from(span_id), 8)),
            ("name".into(), Value::from(name)),
        ]))
    }

    fn envelope(spans: Vec<Value>) -> TraceEvent {
        let scope = Value::Object(ObjectMap::from([("spans".into(), Value::Array(spans))]));
        let resource = Value::Object(ObjectMap::from([(
            "scopeSpans".into(),
            Value::Array(vec![scope]),
        )]));
        let mut event = TraceEvent::default();
        event
            .metadata_mut()
            .set_trace_layout(TraceLayout::OtlpResourceSpans);
        event.insert(event_path!("resourceSpans"), Value::Array(vec![resource]));
        event
    }

    fn encode(event: TraceEvent) -> vector_common::Result<ExportTraceServiceRequest> {
        let mut serializer = OtlpSerializer::new()?;
        let mut buffer = BytesMut::new();
        serializer.encode(Event::Trace(event), &mut buffer)?;
        Ok(ExportTraceServiceRequest::decode(buffer.freeze())?)
    }

    fn span_names(request: &ExportTraceServiceRequest) -> Vec<&str> {
        request
            .resource_spans
            .iter()
            .flat_map(|resource| &resource.scope_spans)
            .flat_map(|scope| &scope.spans)
            .map(|span| span.name.as_str())
            .collect()
    }

    fn counter<'a>(
        metrics: &'a [vector_core::event::Metric],
        name: &str,
    ) -> Option<(&'a vector_core::event::Metric, f64)> {
        let metric = metrics.iter().find(|metric| metric.name() == name)?;
        let vector_core::event::MetricValue::Counter { value } = metric.value() else {
            panic!("{name} is a counter");
        };
        Some((metric, *value))
    }

    fn reset_metrics() -> &'static vector_core::metrics::Controller {
        use vector_core::metrics::{Controller, init_test};

        init_test();
        let controller = Controller::get().expect("metrics controller");
        controller.reset();
        controller
    }

    #[test]
    fn encodes_accepted_spans_when_a_sibling_span_is_rejected() {
        let request = encode(envelope(vec![span("kept", 1, 1), span("dropped", 0, 2)]))
            .expect("a rejected sibling leaves the accepted span encodable");

        assert_eq!(span_names(&request), ["kept"]);
    }

    #[test]
    fn counts_a_rejected_span_as_an_error_and_a_discarded_span() {
        let controller = reset_metrics();
        let request = encode(envelope(vec![span("kept", 1, 1), span("dropped", 0, 2)]))
            .expect("a rejected sibling leaves the accepted span encodable");
        assert_eq!(span_names(&request), ["kept"]);

        let metrics = controller.capture_metrics();
        let (error, value) = counter(&metrics, "component_errors_total")
            .expect("rejected span increments component_errors_total");
        let tags = error.tags().expect("component_errors_total has tags");
        assert_eq!(tags.get("error_code"), Some("invalid_trace_id"));
        assert_eq!(tags.get("error_type"), Some("conversion_failed"));
        assert_eq!(tags.get("stage"), Some("sending"));
        assert!((value - 1.0).abs() < f64::EPSILON);
        let (_, discarded) = counter(&metrics, "component_discarded_events_total")
            .expect("rejected span increments component_discarded_events_total");
        assert!((discarded - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn counts_a_malformed_grouping_as_a_discarded_span() {
        let controller = reset_metrics();
        let mut event = envelope(vec![span("kept", 1, 1)]);
        let Some(Value::Array(resources)) = event.get_mut(event_path!("resourceSpans")) else {
            panic!("envelope has a resourceSpans array");
        };
        resources.push(Value::from(42));

        let request = encode(event).expect("a malformed grouping leaves its sibling encodable");
        assert_eq!(span_names(&request), ["kept"]);

        let metrics = controller.capture_metrics();
        let (error, value) = counter(&metrics, "component_errors_total")
            .expect("malformed grouping increments component_errors_total");
        assert_eq!(
            error.tags().and_then(|tags| tags.get("error_code")),
            Some("discarded_span")
        );
        assert!((value - 1.0).abs() < f64::EPSILON);
        let (_, discarded) = counter(&metrics, "component_discarded_events_total")
            .expect("malformed grouping increments component_discarded_events_total");
        assert!((discarded - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn records_an_unmapped_field_without_dropping_the_span() {
        let controller = reset_metrics();
        let mut kept = span("kept", 1, 1);
        let Value::Object(fields) = &mut kept else {
            panic!("span is an object");
        };
        fields.insert("not_a_field".into(), Value::from(true));

        let request =
            encode(envelope(vec![kept])).expect("an unmapped field leaves the span encodable");
        assert_eq!(span_names(&request), ["kept"]);

        let metrics = controller.capture_metrics();
        let (error, value) = counter(&metrics, "component_errors_total")
            .expect("unmapped field increments component_errors_total");
        assert_eq!(
            error.tags().and_then(|tags| tags.get("error_code")),
            Some("legacy_layout_error")
        );
        assert!((value - 1.0).abs() < f64::EPSILON);
        assert!(counter(&metrics, "component_discarded_events_total").is_none());
    }

    #[test]
    fn returns_ok_when_every_span_is_rejected() {
        let request = encode(envelope(vec![span("dropped", 0, 1)]))
            .expect("rejecting every span consumes the event without failing it");
        assert!(span_names(&request).is_empty());
    }

    #[test]
    fn returns_err_when_the_legacy_layout_is_not_otlp() {
        let mut event = TraceEvent::default();
        event.metadata_mut().set_trace_layout(TraceLayout::Datadog);
        event.insert(event_path!("trace_id"), Value::from("1"));

        assert!(encode(event).is_err());
    }
}
