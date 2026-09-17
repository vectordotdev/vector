//! Datadog Stage 1 legacy layout → typed [`TraceEvent`].
//!
//! Compiles for the `datadog_agent` source or the `datadog_traces` sink. The source
//! continues to emit the existing per-chunk map; this module is the shim that
//! reconstructs the typed grouping native ingest will use after the flip.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use tracing::debug;
use vector_core::event::{
    TraceEvent as LegacyTraceEvent, TraceLayout,
    typed_trace::{
        AttrValue, Attributes, DatadogAgentEnvelope, DatadogChunkContext, DatadogEventContext,
        DatadogSpanContext, DroppedCount, OtlpSpanFlags, Resource, SamplingPriority, Span,
        SpanEvent, SpanId, SpanLink, SpanStatus, TraceEvent, TraceId, TraceState,
    },
};
use vrl::path::PathPrefix;
use vrl::value::{KeyString, ObjectMap, Value};

const TID_TAG: &str = "_dd.p.tid";
const ERROR_MESSAGE: &str = "error.message";
const CONTAINER_ID: &str = "container.id";
const TELEMETRY_SDK_LANGUAGE: &str = "telemetry.sdk.language";
const PROCESS_RUNTIME_VERSION: &str = "process.runtime.version";
const TELEMETRY_SDK_VERSION: &str = "telemetry.sdk.version";
const SERVICE_VERSION: &str = "service.version";
const RUNTIME_ID: &str = "runtime-id";

/// Failure converting a legacy Datadog-shaped trace event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversionError {
    /// The event is not a recognized Datadog historical layout.
    NotDatadogLayout,
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotDatadogLayout => {
                f.write_str("legacy trace event is not a recognized Datadog layout")
            }
        }
    }
}

impl std::error::Error for ConversionError {}

/// Returns `true` when `legacy` matches a historical Datadog Agent chunk layout.
#[must_use]
pub fn is_historical_datadog_layout(legacy: &LegacyTraceEvent) -> bool {
    if let Some(map) = legacy.value().as_object()
        && let Some(Value::Array(spans)) = map.get("spans")
    {
        has_datadog_chunk_fields(map) || spans.iter().any(looks_like_datadog_span)
    } else {
        false
    }
}

fn has_datadog_chunk_fields(map: &ObjectMap) -> bool {
    map.contains_key("priority")
        || map.contains_key("language_name")
        || map.contains_key("payload_version")
        || map.contains_key("dropped")
}

fn looks_like_datadog_span(value: &Value) -> bool {
    if let Some(span) = value.as_object() {
        span.contains_key("meta")
            && (integer_id(span.get("trace_id")).is_some()
                || integer_id(span.get("span_id")).is_some())
    } else {
        false
    }
}

/// Converts a Datadog historical chunk into typed events.
///
/// Partitioning is by reconstructed [`TraceId`] and `Span.service`, in first-seen
/// pair order. Empty `spans` is success with zero outputs.
///
/// # Errors
///
/// Returns [`ConversionError::NotDatadogLayout`] when the record is not a Datadog chunk.
pub fn legacy_to_typed(legacy: &LegacyTraceEvent) -> Result<Vec<TraceEvent>, ConversionError> {
    if is_historical_datadog_layout(legacy) {
        Ok(convert_chunk(legacy))
    } else {
        Err(ConversionError::NotDatadogLayout)
    }
}

/// Converts when the Stage 1 layout hint identifies a Datadog producer.
///
/// An unrecognized present hint is not converted here.
#[must_use]
pub fn hinted_legacy_to_typed(
    legacy: &LegacyTraceEvent,
) -> Option<Result<Vec<TraceEvent>, ConversionError>> {
    match legacy.metadata().trace_layout() {
        Some(TraceLayout::Datadog) => Some(legacy_to_typed(legacy)),
        Some(_) | None => None,
    }
}

fn convert_chunk(legacy: &LegacyTraceEvent) -> Vec<TraceEvent> {
    let Some(map) = legacy.value().as_object() else {
        return Vec::new();
    };
    let incoming = incoming_spans(map);
    let resolved = resolve_trace_ids(incoming);
    let groups = partition_first_seen(resolved);
    let envelope = ChunkEnvelope::from_event(legacy, map);
    groups
        .into_iter()
        .map(|((trace_id, service), spans)| {
            envelope
                .clone()
                .into_trace_event(trace_id, service, spans, legacy.metadata().clone())
        })
        .collect()
}

struct IncomingSpan {
    low: u64,
    high: Option<u64>,
    service: String,
    span: Span,
}

#[derive(Clone)]
struct ChunkEnvelope {
    resource: Resource,
    datadog: DatadogEventContext,
}

impl ChunkEnvelope {
    fn from_event(legacy: &LegacyTraceEvent, map: &ObjectMap) -> Self {
        let chunk = chunk_context(map);
        // `AgentPayload.hostName` is the log-schema host. `TracerPayload.hostname` is the
        // literal `hostname` field, which the legacy encoder reads separately.
        let agent_host = schema_host(legacy);
        Self {
            resource: Resource {
                service: None,
                environment: string_field(map, "env"),
                host: string_field(map, "hostname"),
                attributes: tracer_resource_attributes(map),
                schema_url: None,
                dropped_attributes_count: DroppedCount::ZERO,
            },
            datadog: {
                let mut datadog = DatadogEventContext::default();
                // The source merges tracer and agent tags into the chunk map. The legacy
                // encoder copies that map onto both `TracerPayload.tags` and `TraceChunk.tags`.
                datadog.tracer.tags = chunk.tags.clone();
                datadog.set_agent(Some(agent_envelope(map, agent_host)));
                datadog.set_chunk(Some(chunk));
                datadog
            },
        }
    }

    /// `TraceEvent::from_group` is blocked: `TraceEvent` is defined in `vector_core`.
    fn into_trace_event(
        mut self,
        trace_id: TraceId,
        service: String,
        spans: Vec<Span>,
        metadata: vector_core::event::EventMetadata,
    ) -> TraceEvent {
        self.resource.service = Some(service);
        let mut event = TraceEvent::new_with_metadata(trace_id, metadata);
        *event.resource_mut() = self.resource;
        *event.datadog_mut() = self.datadog;
        *event.spans_mut() = spans;
        event
    }
}

fn incoming_spans(map: &ObjectMap) -> Vec<IncomingSpan> {
    map.get("spans")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(IncomingSpan::from_value)
        .collect()
}

impl IncomingSpan {
    fn from_value(value: &Value) -> Option<Self> {
        let span = value.as_object()?;
        let low = integer_id(span.get("trace_id")).unwrap_or(0);
        let mut meta = object_field(span, "meta").cloned().unwrap_or_default();
        let high = high_from_tid(&meta)
            .inspect_err(|()| {
                debug!(message = "Dropping Datadog span with malformed _dd.p.tid.");
            })
            .ok()?;
        meta.remove(TID_TAG);

        let span_id = SpanId::new(integer_id(span.get("span_id")).unwrap_or(0))
            .inspect_err(|_| {
                debug!(message = "Dropping Datadog span with zero span id.");
            })
            .ok()?;

        Some(Self {
            low,
            high,
            service: string_field(span, "service").unwrap_or_default(),
            span: span_from_datadog_fields(span_id, span, &meta),
        })
    }
}

fn span_from_datadog_fields(span_id: SpanId, map: &ObjectMap, meta: &ObjectMap) -> Span {
    let error = integer_id(map.get("error")).unwrap_or(0);
    let error_message = string_field(meta, ERROR_MESSAGE).unwrap_or_default();
    let status = if error == 0 {
        SpanStatus::Unset
    } else {
        SpanStatus::Error(error_message)
    };

    let (attributes, dropped_attributes_count) = merge_meta_metrics(meta, map.get("metrics"));
    let (events, dropped_events_count) = span_events(map.get("span_events"));
    let (links, dropped_links_count) = span_links(map.get("span_links"));
    Span {
        parent_span_id: SpanId::new(integer_id(map.get("parent_id")).unwrap_or(0)).ok(),
        start_time: timestamp_field(map, "start").unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        duration: duration_field(map.get("duration")),
        status,
        datadog: DatadogSpanContext {
            resource_name: string_field(map, "resource"),
            span_type: string_field(map, "type"),
            meta_struct: meta_struct_field(map.get("meta_struct")),
        },
        attributes,
        events,
        links,
        dropped_attributes_count,
        dropped_events_count,
        dropped_links_count,
        ..Span::new(span_id, string_field(map, "name").unwrap_or_default())
    }
}

fn high_from_tid(meta: &ObjectMap) -> Result<Option<u64>, ()> {
    let Some(value) = meta.get(TID_TAG) else {
        return Ok(None);
    };
    let text = value.to_string_lossy();
    if text.is_empty() {
        return Ok(Some(0));
    }
    parse_hex_u64(&text).map(Some).ok_or(())
}

fn resolve_trace_ids(spans: Vec<IncomingSpan>) -> Vec<(TraceId, IncomingSpan)> {
    let tagged_by_low = tagged_ids_by_low(&spans);
    spans
        .into_iter()
        .filter_map(|incoming| {
            let trace_id = assign_trace_id(&incoming, &tagged_by_low)?;
            Some((trace_id, incoming))
        })
        .collect()
}

fn tagged_ids_by_low(spans: &[IncomingSpan]) -> HashMap<u64, HashSet<u128>> {
    let mut tagged: HashMap<u64, HashSet<u128>> = HashMap::new();
    for span in spans {
        if let Some(high) = span.high {
            let combined = (u128::from(high) << 64) | u128::from(span.low);
            tagged.entry(span.low).or_default().insert(combined);
        }
    }
    tagged
}

fn assign_trace_id(
    span: &IncomingSpan,
    tagged_by_low: &HashMap<u64, HashSet<u128>>,
) -> Option<TraceId> {
    let tagged = tagged_by_low.get(&span.low);
    let combined = match (span.high, tagged) {
        (Some(high), _) => (u128::from(high) << 64) | u128::from(span.low),
        (None, Some(ids)) if ids.len() == 1 => *ids.iter().next().expect("len == 1"),
        (None, Some(_)) => {
            debug!(
                message = "Dropping Datadog span that omitted _dd.p.tid while siblings reconstructed to more than one 128-bit id."
            );
            return None;
        }
        (None, None) => u128::from(span.low),
    };
    TraceId::new(combined)
        .inspect_err(|_| {
            debug!(message = "Dropping Datadog span with a combined zero trace id.");
        })
        .ok()
}

fn partition_first_seen(
    spans: Vec<(TraceId, IncomingSpan)>,
) -> IndexMap<(TraceId, String), Vec<Span>> {
    let mut groups: IndexMap<(TraceId, String), Vec<Span>> = IndexMap::new();
    for (trace_id, incoming) in spans {
        groups
            .entry((trace_id, incoming.service))
            .or_default()
            .push(incoming.span);
    }
    groups
}

fn tracer_resource_attributes(map: &ObjectMap) -> Attributes {
    let mut attributes = Attributes::new();
    attributes.insert_nonempty(CONTAINER_ID, map.get("container_id").map(value_to_string));
    attributes.insert_nonempty(
        TELEMETRY_SDK_LANGUAGE,
        map.get("language_name").map(value_to_string),
    );
    attributes.insert_nonempty(
        PROCESS_RUNTIME_VERSION,
        map.get("language_version").map(value_to_string),
    );
    attributes.insert_nonempty(
        TELEMETRY_SDK_VERSION,
        map.get("tracer_version").map(value_to_string),
    );
    attributes.insert_nonempty(RUNTIME_ID, map.get("runtime_id").map(value_to_string));
    attributes.insert_nonempty(SERVICE_VERSION, map.get("app_version").map(value_to_string));
    attributes
}

fn agent_envelope(map: &ObjectMap, host_name: Option<String>) -> DatadogAgentEnvelope {
    DatadogAgentEnvelope {
        host_name: host_name.unwrap_or_default(),
        env: string_field(map, "env").unwrap_or_default(),
        agent_version: string_field(map, "agent_version").unwrap_or_default(),
        target_tps: float_field(map.get("target_tps")).unwrap_or(0.0),
        error_tps: float_field(map.get("error_tps")).unwrap_or(0.0),
        rare_sampler_enabled: map
            .get("rare_sampler_enabled")
            .and_then(Value::as_boolean)
            .unwrap_or(false),
        tags: Attributes::new(),
    }
}

fn schema_host(legacy: &LegacyTraceEvent) -> Option<String> {
    vector_core::config::log_schema()
        .host_key()
        .and_then(|key| legacy.get((PathPrefix::Event, key)))
        .map(value_to_string)
}

fn chunk_context(map: &ObjectMap) -> DatadogChunkContext {
    // Absent priority stays `None`. Grouping encodes that as AutoKeep (wire 1),
    // matching the legacy sink. A present 0 is an explicit AutoReject.
    let priority = integer_id(map.get("priority")).map(|priority| {
        SamplingPriority::from_i32(
            u32::try_from(priority & u64::from(u32::MAX))
                .expect("priority was masked to u32")
                .cast_signed(),
        )
    });

    DatadogChunkContext {
        priority,
        origin: string_field(map, "origin"),
        dropped: map
            .get("dropped")
            .and_then(Value::as_boolean)
            .unwrap_or(false),
        // The legacy chunk map has no sampler decision. Span meta may still carry `_dd.p.dm`.
        sampling_mechanism: None,
        tags: string_map_attributes(map.get("tags")),
    }
}

fn merge_meta_metrics(meta: &ObjectMap, metrics: Option<&Value>) -> (Attributes, DroppedCount) {
    let mut attributes = Attributes::new();
    let mut dropped = DroppedCount::ZERO;
    for (key, value) in meta {
        attributes.insert(key.clone(), AttrValue::String(value_to_string(value)));
    }
    if let Some(metrics) = metrics.and_then(Value::as_object) {
        for (key, value) in metrics {
            let Some(float) = float_field(Some(value)) else {
                continue;
            };
            if attributes
                .insert(key.clone(), AttrValue::Float(float))
                .is_some()
            {
                dropped.increment(1);
                debug!(
                    message = "Datadog metrics key collided with meta; metrics won.",
                    %key
                );
            }
        }
    }
    (attributes, dropped)
}

fn meta_struct_field(value: Option<&Value>) -> std::collections::BTreeMap<KeyString, Bytes> {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| match value {
            Value::Bytes(bytes) => Some((key.clone(), bytes.clone())),
            _ => None,
        })
        .collect()
}

fn span_events(value: Option<&Value>) -> (Vec<SpanEvent>, DroppedCount) {
    let Some(array) = value.and_then(Value::as_array) else {
        return (Vec::new(), DroppedCount::ZERO);
    };
    let events = array.iter().filter_map(span_event_from_datadog).collect();
    (events, DroppedCount::ZERO)
}

fn span_event_from_datadog(value: &Value) -> Option<SpanEvent> {
    let map = value.as_object()?;
    let (attributes, dropped_attributes_count) = event_attributes(map.get("attributes"));
    Some(SpanEvent {
        name: string_field(map, "name").unwrap_or_default(),
        time: timestamp_field(map, "time_unix_nano").unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        attributes,
        dropped_attributes_count,
    })
}

fn event_attributes(value: Option<&Value>) -> (Attributes, DroppedCount) {
    let mut attributes = Attributes::new();
    let mut dropped = DroppedCount::ZERO;
    let Some(map) = value.and_then(Value::as_object) else {
        return (attributes, dropped);
    };
    for (key, member) in map {
        if let Some(attr) = attr_from_event_value(member) {
            attributes.insert(key.clone(), attr);
        } else {
            dropped.increment(1);
            debug!(
                message = "Dropping unrepresentable Datadog span event attribute.",
                %key
            );
        }
    }
    (attributes, dropped)
}

fn attr_from_event_value(value: &Value) -> Option<AttrValue> {
    match value {
        Value::Bytes(_) | Value::String(_) => Some(AttrValue::String(value_to_string(value))),
        Value::Boolean(v) => Some(AttrValue::Bool(*v)),
        Value::Integer(v) => Some(AttrValue::Integer(*v)),
        Value::Float(v) => Some(AttrValue::Float(v.into_inner())),
        Value::Timestamp(ts) => Some(AttrValue::Integer(
            ts.timestamp_nanos_opt().unwrap_or_default(),
        )),
        Value::Null | Value::Object(_) | Value::Regex(_) => None,
        Value::Array(items) => Some(AttrValue::Array(
            items
                .iter()
                .map(attr_from_array_element)
                .collect::<Option<Vec<AttrValue>>>()?,
        )),
    }
}

fn attr_from_array_element(value: &Value) -> Option<AttrValue> {
    match value {
        Value::Object(_) | Value::Array(_) | Value::Null => None,
        other => attr_from_event_value(other),
    }
}

fn span_links(value: Option<&Value>) -> (Vec<SpanLink>, DroppedCount) {
    let Some(array) = value.and_then(Value::as_array) else {
        return (Vec::new(), DroppedCount::ZERO);
    };
    let mut dropped = DroppedCount::ZERO;
    let links = array
        .iter()
        .filter_map(|item| {
            span_link_from_datadog(item).or_else(|| {
                dropped.increment(1);
                debug!(message = "Dropping Datadog span link with a zero identifier.");
                None
            })
        })
        .collect();
    (links, dropped)
}

fn span_link_from_datadog(value: &Value) -> Option<SpanLink> {
    let map = value.as_object()?;
    let low = decode_u64(map.get("trace_id"));
    let high = decode_u64(map.get("trace_id_high"));
    let trace_id = TraceId::from_datadog(low, high).ok()?;
    let span_id = SpanId::new(decode_u64(map.get("span_id"))).ok()?;
    Some(SpanLink {
        trace_id,
        span_id,
        trace_state: TraceState::from_raw(string_field(map, "tracestate").unwrap_or_default()),
        flags: OtlpSpanFlags::from(
            u32::try_from(integer_id(map.get("flags")).unwrap_or(0) & u64::from(u32::MAX))
                .unwrap_or_default(),
        ),
        attributes: string_map_attributes(map.get("attributes")),
        dropped_attributes_count: DroppedCount::ZERO,
    })
}

fn string_map_attributes(value: Option<&Value>) -> Attributes {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(key, member)| (key.clone(), AttrValue::String(value_to_string(member))))
        .collect()
}

fn integer_id(value: Option<&Value>) -> Option<u64> {
    match value {
        Some(Value::Integer(v)) => Some(v.cast_unsigned()),
        Some(other) => parse_hex_u64(&other.to_string_lossy()),
        None => None,
    }
}

fn parse_hex_u64(text: &str) -> Option<u64> {
    u64::from_str_radix(text, 16).ok()
}

fn decode_u64(value: Option<&Value>) -> u64 {
    integer_id(value).unwrap_or(0)
}

fn string_field(map: &ObjectMap, key: &str) -> Option<String> {
    map.get(key).map(value_to_string)
}

fn object_field<'a>(map: &'a ObjectMap, key: &str) -> Option<&'a ObjectMap> {
    map.get(key).and_then(Value::as_object)
}

fn value_to_string(value: &Value) -> String {
    value.to_string_lossy().into_owned()
}

fn timestamp_field(map: &ObjectMap, key: &str) -> Option<DateTime<Utc>> {
    match map.get(key) {
        Some(Value::Timestamp(ts)) => Some(*ts),
        _ => None,
    }
}

const fn duration_field(value: Option<&Value>) -> Duration {
    match value {
        Some(Value::Integer(n)) if *n < 0 => Duration::ZERO,
        Some(Value::Integer(n)) => Duration::from_nanos(n.cast_unsigned()),
        _ => Duration::ZERO,
    }
}

fn float_field(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Float(v)) => Some(v.into_inner()),
        #[allow(clippy::cast_precision_loss)] // Datadog integers are represented as floats here.
        Some(Value::Integer(v)) => Some(*v as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use similar_asserts::assert_eq;
    use vector_core::event::{
        BatchNotifier, BatchStatus, EventFinalizer, EventStatus, Finalizable, TraceLayout,
    };

    use ordered_float::NotNan;

    use super::*;

    fn from_datadog_chunk(spans: Vec<Value>) -> LegacyTraceEvent {
        let mut event = LegacyTraceEvent::from(ObjectMap::from([
            ("priority".into(), Value::from(1i64)),
            ("origin".into(), Value::from("")),
            ("dropped".into(), Value::from(false)),
            ("tags".into(), Value::Object(ObjectMap::new())),
            ("spans".into(), Value::Array(spans)),
            ("language_name".into(), Value::from("rust")),
            ("env".into(), Value::from("prod")),
            ("host".into(), Value::from("box")),
        ]));
        event.metadata_mut().set_trace_layout(TraceLayout::Datadog);
        event
    }

    fn datadog_span(service: &str, low: u64, span_id: u64, name: &str) -> Value {
        Value::from(ObjectMap::from([
            ("service".into(), Value::from(service)),
            ("name".into(), Value::from(name)),
            ("resource".into(), Value::from(name)),
            ("trace_id".into(), Value::from(low.cast_signed())),
            ("span_id".into(), Value::from(span_id.cast_signed())),
            ("parent_id".into(), Value::from(0i64)),
            ("start".into(), Value::from(Utc.timestamp_nanos(1_000))),
            ("duration".into(), Value::from(100i64)),
            ("error".into(), Value::from(0i64)),
            ("meta".into(), Value::Object(ObjectMap::new())),
            ("metrics".into(), Value::Object(ObjectMap::new())),
            ("type".into(), Value::from("web")),
            ("meta_struct".into(), Value::Object(ObjectMap::new())),
            ("span_links".into(), Value::Array(Vec::new())),
            ("span_events".into(), Value::Array(Vec::new())),
        ]))
    }

    fn object_mut(value: &mut Value) -> &mut ObjectMap {
        match value {
            Value::Object(map) => map,
            _ => panic!("expected object"),
        }
    }

    fn with_tid(mut span: Value, high: u64) -> Value {
        let meta = object_mut(object_mut(&mut span).get_mut("meta").expect("meta"));
        meta.insert(TID_TAG.into(), Value::from(format!("{high:016x}")));
        span
    }

    fn with_service_meta(mut span: Value, key: &str, value: &str) -> Value {
        let meta = object_mut(object_mut(&mut span).get_mut("meta").expect("meta"));
        meta.insert(key.into(), Value::from(value));
        span
    }

    #[test]
    fn empty_chunk_is_success_with_zero_events() {
        let legacy = from_datadog_chunk(Vec::new());
        let typed = legacy_to_typed(&legacy).unwrap();
        assert_eq!(typed, Vec::new());
    }

    #[test]
    fn absent_priority_stays_unset_and_chunk_tags_are_tracer_tags() {
        let mut legacy = from_datadog_chunk(vec![datadog_span("api", 1, 10, "root")]);
        let map = object_mut(legacy.value_mut());
        map.remove("priority");
        map.insert(
            "tags".into(),
            Value::Object(ObjectMap::from([("env".into(), Value::from("prod"))])),
        );

        let typed = legacy_to_typed(&legacy).unwrap();
        let datadog = typed[0].datadog();

        assert_eq!(datadog.chunk().and_then(|chunk| chunk.priority), None);
        assert_eq!(
            datadog.agent().map(|agent| agent.host_name.as_str()),
            Some("box")
        );
        assert_eq!(typed[0].resource().host.as_deref(), None);
        assert_eq!(
            datadog.chunk().and_then(|chunk| chunk.tags.get("env")),
            datadog.tracer.tags.get("env")
        );
        assert!(datadog.tracer.tags.get("env").is_some());
    }

    #[test]
    fn preserves_signed_sampling_priority_bits() {
        let mut legacy = from_datadog_chunk(vec![datadog_span("api", 1, 10, "root")]);
        object_mut(legacy.value_mut()).insert("priority".into(), Value::from("ffffffff"));

        let typed = legacy_to_typed(&legacy).unwrap();

        assert_eq!(
            typed[0].datadog().chunk().and_then(|chunk| chunk.priority),
            Some(SamplingPriority::UserReject)
        );
    }

    #[test]
    fn partitions_by_trace_and_service_first_seen() {
        let legacy = from_datadog_chunk(vec![
            datadog_span("api", 1, 10, "a1"),
            datadog_span("web", 1, 11, "w1"),
            datadog_span("api", 2, 20, "a2"),
            datadog_span("api", 1, 12, "a3"),
        ]);
        let typed = legacy_to_typed(&legacy).unwrap();
        assert_eq!(typed.len(), 3);
        assert_eq!(typed[0].trace_id(), TraceId::from_datadog(1, 0).unwrap());
        assert_eq!(typed[0].resource().service.as_deref(), Some("api"));
        assert_eq!(
            typed[0]
                .spans()
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["a1", "a3"]
        );
        assert_eq!(typed[1].resource().service.as_deref(), Some("web"));
        assert_eq!(typed[2].trace_id(), TraceId::from_datadog(2, 0).unwrap());
    }

    #[test]
    fn reconstructs_128_bit_id_and_consumes_tid_tag() {
        let span = with_tid(datadog_span("api", 1, 10, "root"), 0xabc);
        let legacy = from_datadog_chunk(vec![span]);
        let typed = legacy_to_typed(&legacy).unwrap();
        assert_eq!(
            typed[0].trace_id(),
            TraceId::from_datadog(1, 0xabc).unwrap()
        );
        assert!(typed[0].spans()[0].attributes.get(TID_TAG).is_none());
    }

    #[test]
    fn recovers_low_64_from_i64_bit_pattern() {
        let span = datadog_span("api", u64::MAX, 10, "root");
        let legacy = from_datadog_chunk(vec![span]);
        let typed = legacy_to_typed(&legacy).unwrap();
        assert_eq!(
            typed[0].trace_id(),
            TraceId::from_datadog(u64::MAX, 0).unwrap()
        );
    }

    #[test]
    fn untagged_siblings_inherit_unique_tagged_id() {
        let tagged = with_tid(datadog_span("api", 7, 1, "a"), 5);
        let untagged = datadog_span("api", 7, 2, "b");
        let legacy = from_datadog_chunk(vec![tagged, untagged]);
        let typed = legacy_to_typed(&legacy).unwrap();
        assert_eq!(typed.len(), 1);
        assert_eq!(typed[0].trace_id(), TraceId::from_datadog(7, 5).unwrap());
        assert_eq!(typed[0].spans().len(), 2);
    }

    #[test]
    fn drops_untagged_when_siblings_reconstruct_to_multiple_ids() {
        let a = with_tid(datadog_span("api", 7, 1, "a"), 1);
        let b = with_tid(datadog_span("api", 7, 2, "b"), 2);
        let untagged = datadog_span("api", 7, 3, "c");
        let legacy = from_datadog_chunk(vec![a, b, untagged]);
        let typed = legacy_to_typed(&legacy).unwrap();
        assert_eq!(typed.len(), 2);
        assert_eq!(typed.iter().map(|e| e.spans().len()).sum::<usize>(), 2);
    }

    #[test]
    fn normalizes_error_status_and_keeps_error_meta() {
        let span = with_service_meta(
            {
                let mut span = datadog_span("api", 1, 10, "root");
                object_mut(&mut span).insert("error".into(), Value::from(2i64));
                span
            },
            ERROR_MESSAGE,
            "boom",
        );
        let typed = legacy_to_typed(&from_datadog_chunk(vec![span])).unwrap();
        assert_eq!(typed[0].spans()[0].status, SpanStatus::Error("boom".into()));
        assert_eq!(
            typed[0].spans()[0].attributes.get(ERROR_MESSAGE),
            Some(&AttrValue::String("boom".into()))
        );
    }

    #[test]
    fn converts_string_event_attributes() {
        let value = Value::String("event".into());

        assert_eq!(
            attr_from_event_value(&value),
            Some(AttrValue::String("event".into()))
        );
    }

    #[test]
    fn metrics_win_meta_collisions() {
        let mut span = with_service_meta(datadog_span("api", 1, 10, "root"), "shared", "s");
        object_mut(&mut span).insert(
            "metrics".into(),
            Value::Object(ObjectMap::from([(
                "shared".into(),
                Value::Float(NotNan::new(1.5).unwrap()),
            )])),
        );
        let typed = legacy_to_typed(&from_datadog_chunk(vec![span])).unwrap();
        assert_eq!(
            typed[0].spans()[0].attributes.get("shared"),
            Some(&AttrValue::Float(1.5))
        );
        assert_eq!(
            typed[0].spans()[0].dropped_attributes_count,
            DroppedCount::new(1)
        );
    }

    #[test]
    fn clones_metadata_and_finalizers_onto_each_partition() {
        let (batch, mut rx) = BatchNotifier::new_with_receiver();
        let mut legacy = from_datadog_chunk(vec![
            datadog_span("api", 1, 10, "a"),
            datadog_span("web", 1, 11, "w"),
        ]);
        legacy.add_finalizer(EventFinalizer::new(batch));
        let mut typed = legacy_to_typed(&legacy).unwrap();
        assert_eq!(typed.len(), 2);
        assert_eq!(typed[0].metadata().finalizers().len(), 1);
        assert_eq!(typed[1].metadata().finalizers().len(), 1);
        assert_eq!(
            typed[0].metadata().trace_layout(),
            Some(TraceLayout::Datadog)
        );

        let first = typed[0].take_finalizers();
        first.update_status(EventStatus::Delivered);
        drop(first);
        assert_eq!(typed[0].metadata().finalizers().len(), 0);
        assert_eq!(typed[1].metadata().finalizers().len(), 1);

        let second = typed[1].take_finalizers();
        second.update_status(EventStatus::Delivered);
        drop(second);
        let leftover = legacy.take_finalizers();
        leftover.update_status(EventStatus::Delivered);
        drop(leftover);
        drop(typed);
        assert_eq!(rx.try_recv(), Ok(BatchStatus::Delivered));
    }

    #[test]
    fn hinted_converter_ignores_other_hints() {
        let mut legacy = from_datadog_chunk(vec![datadog_span("api", 1, 10, "root")]);
        legacy
            .metadata_mut()
            .set_trace_layout(TraceLayout::OtlpResourceSpans);
        assert!(hinted_legacy_to_typed(&legacy).is_none());
    }
}
