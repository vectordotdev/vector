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
#[cfg(not(feature = "typed-trace"))]
use vrl::event_path;

#[cfg(not(feature = "typed-trace"))]
use super::apm_stats::compute_apm_stats;
use super::{
    apm_stats::Aggregator,
    config::{DatadogTracesEndpoint, DatadogTracesEndpointConfiguration},
    dd_proto,
    service::TraceApiRequest,
    sink::PartitionKey,
};
#[cfg(not(feature = "typed-trace"))]
use crate::common::datadog::decode_u64_id;
#[cfg(feature = "typed-trace")]
use crate::event::typed_trace::TraceEvent;
#[cfg(not(feature = "typed-trace"))]
use crate::event::{Event, ObjectMap, TraceEvent, Value};
use crate::sinks::util::{
    Compression, Compressor, IncrementalRequestBuilder, metadata::RequestMetadataBuilder,
};

#[derive(Debug, Snafu)]
pub(crate) enum RequestBuilderError {
    #[snafu(display(
        "Building an APM stats request payload failed ({}, {})",
        message,
        reason
    ))]
    FailedToBuild {
        message: &'static str,
        reason: String,
        dropped_events: usize,
    },

    #[allow(dead_code)]
    #[snafu(display("Unsupported endpoint ({})", reason))]
    UnsupportedEndpoint {
        reason: String,
        dropped_events: usize,
    },
}

impl RequestBuilderError {
    pub(crate) fn into_parts(self) -> (&'static str, String, usize) {
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

pub(super) struct DatadogTracesRequestBuilder {
    api_key: Arc<str>,
    endpoint_configuration: DatadogTracesEndpointConfiguration,
    compression: Compression,
    max_size: usize,
    /// Contains the Aggregated stats across a time window.
    stats_aggregator: Arc<Mutex<Aggregator>>,
    disable_apm_stats: bool,
}

impl DatadogTracesRequestBuilder {
    pub(super) const fn new(
        api_key: Arc<str>,
        endpoint_configuration: DatadogTracesEndpointConfiguration,
        compression: Compression,
        max_size: usize,
        stats_aggregator: Arc<Mutex<Aggregator>>,
        disable_apm_stats: bool,
    ) -> Self {
        Self {
            api_key,
            endpoint_configuration,
            compression,
            max_size,
            stats_aggregator,
            disable_apm_stats,
        }
    }
}

pub(crate) struct DDTracesMetadata {
    pub api_key: Arc<str>,
    pub endpoint: DatadogTracesEndpoint,
    pub finalizers: EventFinalizers,
    pub uncompressed_size: usize,
    pub content_type: String,
}

#[cfg(not(feature = "typed-trace"))]
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
            .filter_map(Event::try_into_trace)
            .collect::<Vec<TraceEvent>>();

        // Compute APM stats from the incoming events. The stats payloads are sent out
        // separately from the sink framework, by the thread `flush_apm_stats_thread()`
        if !self.disable_apm_stats {
            compute_apm_stats(&key, &self.stats_aggregator, &trace_events);
        }

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
                            dropped_events: processed.len(),
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
pub(crate) fn build_request(
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
        uri: endpoint_configuration.get_uri_for_endpoint(ddtraces_metadata.endpoint),
        uncompressed_size: ddtraces_metadata.uncompressed_size,
        metadata: request_metadata,
    }
}

/// An encoded payload and the trace events it contains.
type EncodeResult = Result<(Vec<u8>, Vec<TraceEvent>), RequestBuilderError>;

#[cfg(feature = "typed-trace")]
impl IncrementalRequestBuilder<(PartitionKey, Vec<TraceEvent>)> for DatadogTracesRequestBuilder {
    type Metadata = (DDTracesMetadata, RequestMetadata);
    type Payload = Bytes;
    type Request = TraceApiRequest;
    type Error = RequestBuilderError;

    fn encode_events_incremental(
        &mut self,
        input: (PartitionKey, Vec<TraceEvent>),
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>> {
        let (key, events) = input;
        let mut reporter = super::typed::IssueLog;
        let groups = super::typed::group_typed_traces(events, &mut reporter);
        if !self.disable_apm_stats {
            super::apm_stats::compute_typed_apm_stats(
                key.api_key.as_ref(),
                &self.stats_aggregator,
                &groups,
            );
        }

        encode_typed_groups(&groups, self.max_size)
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
                            dropped_events: processed.len(),
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

#[cfg(feature = "typed-trace")]
fn encode_typed_groups(groups: &[super::typed::AgentGroup], max_size: usize) -> Vec<EncodeResult> {
    groups
        .iter()
        .flat_map(|group| encode_typed_group(group, max_size))
        .collect()
}

#[cfg(feature = "typed-trace")]
fn encode_typed_group(group: &super::typed::AgentGroup, max_size: usize) -> Vec<EncodeResult> {
    let encoded = group.encode();
    if encoded.encoded_len() < max_size {
        return vec![Ok((encoded.encode_to_vec(), group.events.clone()))];
    }

    let mut results = Vec::new();
    let mut payload = empty_agent(group);
    let mut processed = Vec::new();
    let remember = |processed: &mut Vec<TraceEvent>, chunk: &super::typed::ChunkGroup| {
        processed.extend(
            chunk
                .event_indexes
                .iter()
                .map(|index| group.events[*index].clone()),
        );
    };

    for (mut tracer_proto, tracer) in encoded.tracer_payloads.into_iter().zip(&group.tracers) {
        let chunks = std::mem::take(&mut tracer_proto.chunks);
        for (chunk_proto, chunk) in chunks.into_iter().zip(&tracer.chunks) {
            let mut piece = tracer_proto.clone();
            piece.chunks.push(chunk_proto);
            payload.tracer_payloads.push(piece);
            if payload.encoded_len() < max_size {
                remember(&mut processed, chunk);
                continue;
            }

            let oversized = payload.tracer_payloads.pop().expect("just pushed");
            if !payload.tracer_payloads.is_empty() {
                results.push(Ok((
                    payload.encode_to_vec(),
                    std::mem::take(&mut processed),
                )));
                payload = empty_agent(group);
            }
            payload.tracer_payloads.push(oversized);
            if payload.encoded_len() >= max_size {
                payload.tracer_payloads.clear();
                results.push(Err(RequestBuilderError::FailedToBuild {
                    message: "Dropped trace event",
                    reason: "Trace is larger than allowed payload size".into(),
                    dropped_events: chunk.event_indexes.len(),
                }));
            } else {
                remember(&mut processed, chunk);
            }
        }
    }
    if !payload.tracer_payloads.is_empty() {
        results.push(Ok((payload.encode_to_vec(), processed)));
    }
    results
}

#[cfg(feature = "typed-trace")]
fn empty_agent(group: &super::typed::AgentGroup) -> dd_proto::AgentPayload {
    dd_proto::AgentPayload {
        host_name: group.host_name.clone(),
        env: group.env.clone(),
        tracer_payloads: Vec::new(),
        tags: group.tags.clone(),
        agent_version: group.agent_version.clone(),
        target_tps: group.target_tps,
        error_tps: group.error_tps,
        rare_sampler_enabled: group.rare_sampler_enabled,
        idx_tracer_payloads: Vec::new(),
    }
}

#[cfg(not(feature = "typed-trace"))]
fn encode_traces(
    key: &PartitionKey,
    trace_events: Vec<TraceEvent>,
    max_size: usize,
) -> Vec<EncodeResult> {
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
                }
                // try with a fresh payload
                results.push(Ok((
                    payload.encode_to_vec(),
                    std::mem::take(&mut processed),
                )));
                payload = build_empty_payload(key);
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

// https://github.com/vectordotdev/vector/issues/23659
#[allow(clippy::cast_precision_loss, reason = "Sampling rates use f64.")]
#[cfg(not(feature = "typed-trace"))]
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

// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::cast_possible_truncation,
    reason = "Preserve wire priority conversion."
)]
#[cfg(not(feature = "typed-trace"))]
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
            .and_then(Value::as_boolean)
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

// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Preserve wire ID and error conversions."
)]
#[cfg(not(feature = "typed-trace"))]
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

#[cfg(not(feature = "typed-trace"))]
fn convert_span_links(links: &[Value]) -> Vec<dd_proto::SpanLink> {
    links
        .iter()
        .filter_map(|link| link.as_object().map(convert_span_link))
        .collect()
}

#[cfg(not(feature = "typed-trace"))]
fn convert_span_events(events: &[Value]) -> Vec<dd_proto::SpanEvent> {
    events
        .iter()
        .filter_map(|event| event.as_object().map(convert_span_event))
        .collect()
}

// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Preserve wire flag conversion."
)]
#[cfg(not(feature = "typed-trace"))]
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

// https://github.com/vectordotdev/vector/issues/23659
#[allow(clippy::cast_sign_loss, reason = "Preserve wire timestamp conversion.")]
#[cfg(not(feature = "typed-trace"))]
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

#[cfg(not(feature = "typed-trace"))]
fn encode_span_event_attributes(
    attrs: &ObjectMap,
) -> BTreeMap<String, dd_proto::AttributeAnyValue> {
    attrs
        .iter()
        .map(|(k, v)| (k.to_string(), encode_attribute_any_value(v)))
        .collect()
}

#[cfg(not(feature = "typed-trace"))]
fn integer_field(object: &ObjectMap, key: &str) -> i64 {
    match object.get(key) {
        Some(Value::Integer(val)) => *val,
        _ => 0,
    }
}

#[cfg(not(feature = "typed-trace"))]
fn u64_id_field(object: &ObjectMap, key: &str) -> u64 {
    object.get(key).map_or(0, decode_u64_id)
}

#[cfg(not(feature = "typed-trace"))]
fn string_field(object: &ObjectMap, key: &str) -> String {
    object
        .get(key)
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(not(feature = "typed-trace"))]
fn string_map_field(object: &ObjectMap, key: &str) -> BTreeMap<String, String> {
    object
        .get(key)
        .and_then(Value::as_object)
        .map(object_to_string_map)
        .unwrap_or_default()
}

#[cfg(not(feature = "typed-trace"))]
fn object_to_string_map(object: &ObjectMap) -> BTreeMap<String, String> {
    object
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
        .collect()
}

#[cfg(not(feature = "typed-trace"))]
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

#[cfg(not(feature = "typed-trace"))]
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

#[cfg(all(test, not(feature = "typed-trace")))]
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
                        "encoded len {} longer than max size {max_size}",
                        encoded.len()
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

#[cfg(all(test, feature = "typed-trace"))]
mod typed_size {
    use prost::Message;

    use super::{RequestBuilderError, dd_proto, encode_typed_group};
    use crate::{
        event::typed_trace::{
            Span, SpanId, TraceConversionIssue, TraceConversionReporter, TraceEvent, TraceId,
        },
        sinks::datadog::traces::typed::group_typed_traces,
    };

    struct Quiet;

    impl TraceConversionReporter for Quiet {
        fn report(&mut self, _issue: TraceConversionIssue<'_>) {}
    }

    fn traced(trace: u128, span_id: u64, name: &str) -> TraceEvent {
        let mut event = TraceEvent::new(TraceId::new(trace).expect("non-zero"));
        event.resource_mut().service = Some("a".to_owned());
        *event.spans_mut() = vec![Span::new(SpanId::new(span_id).expect("non-zero"), name)];
        event
    }

    #[test]
    fn drops_only_the_chunk_that_alone_exceeds_the_limit() {
        let big = "x".repeat(4_000);
        for (events, kept_index) in [
            (vec![traced(1, 1, "small"), traced(2, 2, &big)], 0),
            (vec![traced(2, 2, &big), traced(1, 1, "small")], 1),
        ] {
            let groups = group_typed_traces(events, &mut Quiet);
            let results = encode_typed_group(&groups[0], 1_000);
            assert_eq!(results.len(), 2);
            let (body, processed) = results[kept_index].as_ref().expect("small chunk fits");
            assert_eq!(processed.len(), 1);
            assert_eq!(processed[0].spans()[0].span_id.get(), 1);
            let payload = dd_proto::AgentPayload::decode(body.as_slice()).unwrap();
            assert_eq!(payload.tracer_payloads[0].chunks[0].spans[0].span_id, 1);
            let Err(error) = &results[1 - kept_index] else {
                panic!("big chunk drops");
            };
            assert!(matches!(
                error,
                RequestBuilderError::FailedToBuild {
                    dropped_events: 1,
                    ..
                }
            ));
        }
    }
}
