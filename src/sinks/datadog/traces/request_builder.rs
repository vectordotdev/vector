use std::{
    collections::BTreeMap,
    io::Write,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use prost::Message;
use snafu::Snafu;
use vector_lib::{
    event::{EventFinalizers, Finalizable},
    request_metadata::RequestMetadata,
};
use vrl::event_path;

use super::{
    apm_stats::{Aggregator, compute_apm_stats},
    config::{DatadogTracesEndpoint, DatadogTracesEndpointConfiguration},
    dd_proto,
    service::TraceApiRequest,
    sink::PartitionKey,
};
use crate::{
    common::datadog::decode_u64_id,
    event::{Event, ObjectMap, TraceEvent, Value},
    sinks::util::{
        Compression, Compressor, IncrementalRequestBuilder, metadata::RequestMetadataBuilder,
    },
};

#[derive(Debug, Snafu)]
pub enum RequestBuilderError {
    #[snafu(display(
        "Building an APM stats request payload failed ({}, {})",
        message,
        reason
    ))]
    FailedToBuild {
        message: &'static str,
        reason: String,
        dropped_events: u64,
    },

    #[allow(dead_code)]
    #[snafu(display("Unsupported endpoint ({})", reason))]
    UnsupportedEndpoint { reason: String, dropped_events: u64 },
}

impl RequestBuilderError {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn into_parts(self) -> (&'static str, String, u64) {
        match self {
            Self::FailedToBuild {
                message,
                reason,
                dropped_events,
            } => (message, reason, dropped_events),
            Self::UnsupportedEndpoint {
                reason,
                dropped_events,
            } => ("unsupported endpoint", reason, dropped_events),
        }
    }
}

pub struct DatadogTracesRequestBuilder {
    api_key: Arc<str>,
    endpoint_configuration: DatadogTracesEndpointConfiguration,
    compression: Compression,
    max_size: usize,
    /// Contains the Aggregated stats across a time window.
    stats_aggregator: Arc<Mutex<Aggregator>>,
}

impl DatadogTracesRequestBuilder {
    pub const fn new(
        api_key: Arc<str>,
        endpoint_configuration: DatadogTracesEndpointConfiguration,
        compression: Compression,
        max_size: usize,
        stats_aggregator: Arc<Mutex<Aggregator>>,
    ) -> Result<Self, RequestBuilderError> {
        Ok(Self {
            api_key,
            endpoint_configuration,
            compression,
            max_size,
            stats_aggregator,
        })
    }
}

pub struct DDTracesMetadata {
    pub api_key: Arc<str>,
    pub endpoint: DatadogTracesEndpoint,
    pub finalizers: EventFinalizers,
    pub uncompressed_size: usize,
    pub content_type: String,
}

impl IncrementalRequestBuilder<(PartitionKey, Vec<Event>)> for DatadogTracesRequestBuilder {
    type Metadata = (DDTracesMetadata, RequestMetadata);
    type Payload = Bytes;
    type Request = TraceApiRequest;
    type Error = RequestBuilderError;

    fn encode_events_incremental(
        &mut self,
        input: (PartitionKey, Vec<Event>),
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>> {
        let (key, events) = input;
        let trace_events = events
            .into_iter()
            .filter_map(|e| e.try_into_trace())
            .collect::<Vec<TraceEvent>>();

        // Compute APM stats from the incoming events. The stats payloads are sent out
        // separately from the sink framework, by the thread `flush_apm_stats_thread()`
        compute_apm_stats(&key, Arc::clone(&self.stats_aggregator), &trace_events);

        encode_traces(&key, trace_events, self.max_size)
            .into_iter()
            .map(|result| {
                result.and_then(|(payload, mut processed)| {
                    let uncompressed_size = payload.len();
                    let metadata = DDTracesMetadata {
                        api_key: key
                            .api_key
                            .clone()
                            .unwrap_or_else(|| Arc::clone(&self.api_key)),
                        endpoint: DatadogTracesEndpoint::Traces,
                        finalizers: processed.take_finalizers(),
                        uncompressed_size,
                        content_type: "application/x-protobuf".to_string(),
                    };

                    // build RequestMetadata
                    let builder = RequestMetadataBuilder::from_events(&processed);

                    let mut compressor = Compressor::from(self.compression);
                    match compressor.write_all(&payload) {
                        Ok(()) => {
                            let bytes = compressor.into_inner().freeze();

                            let bytes_len = NonZeroUsize::new(bytes.len())
                                .expect("payload should never be zero length");
                            let request_metadata = builder.with_request_size(bytes_len);

                            Ok(((metadata, request_metadata), bytes))
                        }
                        Err(e) => Err(RequestBuilderError::FailedToBuild {
                            message: "Payload compression failed.",
                            reason: e.to_string(),
                            dropped_events: processed.len() as u64,
                        }),
                    }
                })
            })
            .collect()
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        build_request(
            metadata,
            payload,
            self.compression,
            &self.endpoint_configuration,
        )
    }
}

/// Builds the `TraceApiRequest` from inputs.
///
/// # Arguments
///
/// * `metadata`                 - Tuple of Datadog traces specific metadata and the generic `RequestMetadata`.
/// * `payload`                  - Compressed and encoded bytes to send.
/// * `compression`              - `Compression` used to reference the Content-Encoding header.
/// * `endpoint_configuration`   - Endpoint configuration to use when creating the HTTP requests.
pub fn build_request(
    metadata: (DDTracesMetadata, RequestMetadata),
    payload: Bytes,
    compression: Compression,
    endpoint_configuration: &DatadogTracesEndpointConfiguration,
) -> TraceApiRequest {
    let (ddtraces_metadata, request_metadata) = metadata;
    let mut headers = BTreeMap::<String, String>::new();
    headers.insert("Content-Type".to_string(), ddtraces_metadata.content_type);
    headers.insert(
        "DD-API-KEY".to_string(),
        ddtraces_metadata.api_key.to_string(),
    );
    if let Some(ce) = compression.content_encoding() {
        headers.insert("Content-Encoding".to_string(), ce.to_string());
    }
    TraceApiRequest {
        body: payload,
        headers,
        finalizers: ddtraces_metadata.finalizers,
        uri: endpoint_configuration
            .get_uri_for_endpoint(ddtraces_metadata.endpoint)
            .into_uri(),
        uncompressed_size: ddtraces_metadata.uncompressed_size,
        metadata: request_metadata,
    }
}

fn encode_traces(
    key: &PartitionKey,
    trace_events: Vec<TraceEvent>,
    max_size: usize,
) -> Vec<Result<(Vec<u8>, Vec<TraceEvent>), RequestBuilderError>> {
    let mut results = Vec::new();
    let mut processed = Vec::new();
    let mut payload = build_empty_payload(key);

    for trace in trace_events {
        let mut proto = encode_trace(&trace);

        loop {
            payload.tracer_payloads.push(proto);
            if payload.encoded_len() >= max_size {
                // take it back out
                proto = payload.tracer_payloads.pop().expect("just pushed");
                if payload.tracer_payloads.is_empty() {
                    // this individual trace is too big
                    results.push(Err(RequestBuilderError::FailedToBuild {
                        message: "Dropped trace event",
                        reason: "Trace is larger than allowed payload size".into(),
                        dropped_events: 1,
                    }));

                    break;
                } else {
                    // try with a fresh payload
                    results.push(Ok((
                        payload.encode_to_vec(),
                        std::mem::take(&mut processed),
                    )));
                    payload = build_empty_payload(key);
                }
            } else {
                processed.push(trace);
                break;
            }
        }
    }
    results.push(Ok((
        payload.encode_to_vec(),
        std::mem::take(&mut processed),
    )));
    results
}

fn build_empty_payload(key: &PartitionKey) -> dd_proto::AgentPayload {
    dd_proto::AgentPayload {
        host_name: key.hostname.clone().unwrap_or_default(),
        env: key.env.clone().unwrap_or_default(),
        tracer_payloads: vec![],
        // We only send tags at the Trace level
        tags: BTreeMap::new(),
        agent_version: key.agent_version.clone().unwrap_or_default(),
        target_tps: key.target_tps.map(|tps| tps as f64).unwrap_or_default(),
        error_tps: key.error_tps.map(|tps| tps as f64).unwrap_or_default(),
        rare_sampler_enabled: false,
        idx_tracer_payloads: vec![],
    }
}

fn encode_trace(trace: &TraceEvent) -> dd_proto::TracerPayload {
    let tags = trace
        .get(event_path!("tags"))
        .and_then(|m| m.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default();

    let spans = match trace.get(event_path!("spans")) {
        Some(Value::Array(v)) => v
            .iter()
            .filter_map(|s| s.as_object().map(convert_span))
            .collect(),
        _ => vec![],
    };

    let chunk = dd_proto::TraceChunk {
        priority: trace
            .get(event_path!("priority"))
            .and_then(|v| v.as_integer().map(|v| v as i32))
            // This should not happen for Datadog originated traces, but in case this field is not populated
            // we default to 1 (https://github.com/DataDog/datadog-agent/blob/eac2327/pkg/trace/sampler/sampler.go#L54-L55),
            // which is what the Datadog trace-agent is doing for OTLP originated traces, as per
            // https://github.com/DataDog/datadog-agent/blob/3ea2eb4/pkg/trace/api/otlp.go#L309.
            .unwrap_or(1i32),
        origin: trace
            .get(event_path!("origin"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        dropped_trace: trace
            .get(event_path!("dropped"))
            .and_then(|v| v.as_boolean())
            .unwrap_or(false),
        spans,
        tags: tags.clone(),
    };

    dd_proto::TracerPayload {
        container_id: trace
            .get(event_path!("container_id"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        language_name: trace
            .get(event_path!("language_name"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        language_version: trace
            .get(event_path!("language_version"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        tracer_version: trace
            .get(event_path!("tracer_version"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        runtime_id: trace
            .get(event_path!("runtime_id"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        chunks: vec![chunk],
        tags,
        env: trace
            .get(event_path!("env"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        hostname: trace
            .get(event_path!("hostname"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        app_version: trace
            .get(event_path!("app_version"))
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        container_debug: None,
    }
}

fn convert_span(span: &ObjectMap) -> dd_proto::Span {
    let trace_id = match span.get("trace_id") {
        Some(Value::Integer(val)) => *val,
        _ => 0,
    };
    let span_id = match span.get("span_id") {
        Some(Value::Integer(val)) => *val,
        _ => 0,
    };
    let parent_id = match span.get("parent_id") {
        Some(Value::Integer(val)) => *val,
        _ => 0,
    };
    let duration = match span.get("duration") {
        Some(Value::Integer(val)) => *val,
        _ => 0,
    };
    let error = match span.get("error") {
        Some(Value::Integer(val)) => *val,
        _ => 0,
    };
    let start = match span.get("start") {
        Some(Value::Timestamp(val)) => val.timestamp_nanos_opt().expect("Timestamp out of range"),
        _ => 0,
    };

    let meta = span
        .get("meta")
        .and_then(|m| m.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default();

    let meta_struct = span
        .get("meta_struct")
        .and_then(|m| m.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.to_string(), v.coerce_to_bytes().into_iter().collect()))
                .collect::<BTreeMap<String, Vec<u8>>>()
        })
        .unwrap_or_default();

    let metrics = span
        .get("metrics")
        .and_then(|m| m.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| {
                    if let Value::Float(f) = v {
                        Some((k.to_string(), f.into_inner()))
                    } else {
                        None
                    }
                })
                .collect::<BTreeMap<String, f64>>()
        })
        .unwrap_or_default();

    dd_proto::Span {
        service: span
            .get("service")
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        name: span
            .get("name")
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        resource: span
            .get("resource")
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        r#type: span
            .get("type")
            .map(|v| v.to_string_lossy().into_owned())
            .unwrap_or_default(),
        trace_id: trace_id as u64,
        span_id: span_id as u64,
        parent_id: parent_id as u64,
        error: error as i32,
        start,
        duration,
        meta,
        metrics,
        meta_struct,
        span_links: span
            .get("span_links")
            .and_then(Value::as_array)
            .map(convert_span_links)
            .unwrap_or_default(),
        span_events: span
            .get("span_events")
            .and_then(Value::as_array)
            .map(convert_span_events)
            .unwrap_or_default(),
    }
}

fn convert_span_links(links: &[Value]) -> Vec<dd_proto::SpanLink> {
    links
        .iter()
        .filter_map(|link| link.as_object().map(convert_span_link))
        .collect()
}

fn convert_span_events(events: &[Value]) -> Vec<dd_proto::SpanEvent> {
    events
        .iter()
        .filter_map(|event| event.as_object().map(convert_span_event))
        .collect()
}

fn convert_span_link(link: &ObjectMap) -> dd_proto::SpanLink {
    dd_proto::SpanLink {
        trace_id: u64_id_field(link, "trace_id"),
        trace_id_high: u64_id_field(link, "trace_id_high"),
        span_id: u64_id_field(link, "span_id"),
        attributes: string_map_field(link, "attributes"),
        tracestate: string_field(link, "tracestate"),
        flags: integer_field(link, "flags") as u32,
    }
}

fn convert_span_event(event: &ObjectMap) -> dd_proto::SpanEvent {
    dd_proto::SpanEvent {
        time_unix_nano: match event.get("time_unix_nano") {
            Some(Value::Timestamp(val)) => val.timestamp_nanos_opt().unwrap_or_default() as u64,
            Some(Value::Integer(val)) => *val as u64,
            _ => 0,
        },
        name: string_field(event, "name"),
        attributes: event
            .get("attributes")
            .and_then(Value::as_object)
            .map(encode_span_event_attributes)
            .unwrap_or_default(),
    }
}

fn encode_span_event_attributes(
    attrs: &ObjectMap,
) -> BTreeMap<String, dd_proto::AttributeAnyValue> {
    attrs
        .iter()
        .map(|(k, v)| (k.to_string(), encode_attribute_any_value(v)))
        .collect()
}

fn integer_field(object: &ObjectMap, key: &str) -> i64 {
    match object.get(key) {
        Some(Value::Integer(val)) => *val,
        _ => 0,
    }
}

fn u64_id_field(object: &ObjectMap, key: &str) -> u64 {
    object.get(key).map(decode_u64_id).unwrap_or(0)
}

fn string_field(object: &ObjectMap, key: &str) -> String {
    object
        .get(key)
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn string_map_field(object: &ObjectMap, key: &str) -> BTreeMap<String, String> {
    object
        .get(key)
        .and_then(Value::as_object)
        .map(object_to_string_map)
        .unwrap_or_default()
}

fn object_to_string_map(object: &ObjectMap) -> BTreeMap<String, String> {
    object
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
        .collect()
}

fn encode_attribute_any_value(value: &Value) -> dd_proto::AttributeAnyValue {
    use dd_proto::attribute_any_value::AttributeAnyValueType;

    match value {
        Value::Boolean(v) => dd_proto::AttributeAnyValue {
            r#type: AttributeAnyValueType::BoolValue as i32,
            bool_value: *v,
            ..Default::default()
        },
        Value::Integer(v) => dd_proto::AttributeAnyValue {
            r#type: AttributeAnyValueType::IntValue as i32,
            int_value: *v,
            ..Default::default()
        },
        Value::Float(v) => dd_proto::AttributeAnyValue {
            r#type: AttributeAnyValueType::DoubleValue as i32,
            double_value: v.into_inner(),
            ..Default::default()
        },
        Value::Array(values) => dd_proto::AttributeAnyValue {
            r#type: AttributeAnyValueType::ArrayValue as i32,
            array_value: Some(dd_proto::AttributeArray {
                values: values.iter().map(encode_attribute_array_value).collect(),
            }),
            ..Default::default()
        },
        other => dd_proto::AttributeAnyValue {
            r#type: AttributeAnyValueType::StringValue as i32,
            string_value: other.to_string_lossy().into_owned(),
            ..Default::default()
        },
    }
}

fn encode_attribute_array_value(value: &Value) -> dd_proto::AttributeArrayValue {
    use dd_proto::attribute_array_value::AttributeArrayValueType;

    match value {
        Value::Boolean(v) => dd_proto::AttributeArrayValue {
            r#type: AttributeArrayValueType::BoolValue as i32,
            bool_value: *v,
            ..Default::default()
        },
        Value::Integer(v) => dd_proto::AttributeArrayValue {
            r#type: AttributeArrayValueType::IntValue as i32,
            int_value: *v,
            ..Default::default()
        },
        Value::Float(v) => dd_proto::AttributeArrayValue {
            r#type: AttributeArrayValueType::DoubleValue as i32,
            double_value: v.into_inner(),
            ..Default::default()
        },
        other => dd_proto::AttributeArrayValue {
            r#type: AttributeArrayValueType::StringValue as i32,
            string_value: other.to_string_lossy().into_owned(),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod test {
    use proptest::prelude::*;
    use vrl::event_path;

    use super::{PartitionKey, convert_span_link, encode_traces};
    use crate::event::{LogEvent, ObjectMap, TraceEvent, Value};

    proptest! {
        #[test]
        fn successfully_encode_payloads_smaller_than_max_size(
            // 476 is the experimentally determined size that will fill a payload after encoding and overhead
            lengths in proptest::collection::vec(16usize..476, 1usize..256),
        ) {
            let max_size = 1024;

            let key = PartitionKey {
                api_key: Some("x".repeat(128).into()),
                env: Some("production".into()),
                hostname: Some("foo.bar.baz.local".into()),
                agent_version: Some("1.2.3.4.5".into()),
                target_tps: None,
                error_tps: None,
            };

            // We only care about the size of the incoming traces, so just populate a single tag field
            // that will be copied into the protobuf representation.
            let traces = lengths
                .into_iter()
                .map(|n| {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("tags", "foo"), "x".repeat(n));
                    TraceEvent::from(log)
                })
                .collect();

            for result in encode_traces(&key, traces, max_size) {
                prop_assert!(result.is_ok());
                let (encoded, _processed) = result.unwrap();

                prop_assert!(
                    encoded.len() <= max_size,
                    "encoded len {} longer than max size {}",
                    encoded.len(),
                    max_size
                );
            }
        }
    }

    #[test]
    fn handles_too_large_events() {
        let max_size = 1024;
        // 476 is experimentally determined to be too big to fit into a <1024 byte proto
        let lengths = [128, 476, 128];

        let key = PartitionKey {
            api_key: Some("x".repeat(128).into()),
            env: Some("production".into()),
            hostname: Some("foo.bar.baz.local".into()),
            agent_version: Some("1.2.3.4.5".into()),
            target_tps: None,
            error_tps: None,
        };

        // We only care about the size of the incoming traces, so just populate a single tag field
        // that will be copied into the protobuf representation.
        let traces = lengths
            .into_iter()
            .map(|n| {
                let mut log = LogEvent::default();
                log.insert(event_path!("tags", "foo"), "x".repeat(n));
                TraceEvent::from(log)
            })
            .collect();

        let mut results = encode_traces(&key, traces, max_size);
        assert_eq!(3, results.len());

        match &mut results[..] {
            [Ok(one), Err(_two), Ok(three)] => {
                for (encoded, processed) in [one, three] {
                    assert_eq!(1, processed.len());
                    assert!(
                        encoded.len() <= max_size,
                        "encoded len {} longer than max size {}",
                        encoded.len(),
                        max_size
                    );
                }
            }
            _ => panic!(
                "unexpected output {:?}",
                results
                    .iter()
                    .map(|r| r.as_ref().map(|(_, p)| p.len()))
                    .collect::<Vec<_>>()
            ),
        }
    }

    #[test]
    fn encodes_span_link_hex_ids_above_i64_max() {
        let link = ObjectMap::from([
            ("trace_id".into(), Value::from("ffffffffffffffff")),
            ("trace_id_high".into(), Value::from("8000000000000000")),
            ("span_id".into(), Value::from("deadbeefcafebabe")),
        ]);
        let encoded = convert_span_link(&link);
        assert_eq!(encoded.trace_id, u64::MAX);
        assert_eq!(encoded.trace_id_high, 1u64 << 63);
        assert_eq!(encoded.span_id, 0xdead_beef_cafe_babe);
    }
}
