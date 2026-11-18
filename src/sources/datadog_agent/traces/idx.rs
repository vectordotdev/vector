use std::{cell::Cell, collections::BTreeMap, fmt};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use fixed_decimal::{Decimal, FloatPrecision};
use ordered_float::NotNan;
use serde_json::{Map as JsonMap, Value as JsonValue};
use vrl::{event_path, value};

use super::{super::ddtrace_proto, handler::new_trace_event};
use crate::{
    common::datadog::encode_u64_id_hex,
    event::{ObjectMap, TraceEvent, Value},
};
use ddtrace_proto::idx::{
    AnyValue, InvalidStringRef, SpanKind, StringTable, any_value::Value as IdxValue,
    format_sampling_mechanism, span_kind_name, split_trace_id,
};

#[cfg(test)]
mod tests;

const SAMPLING_MECHANISM_TAG: &str = "_dd.p.dm";
const TRACE_ID_HIGH_TAG: &str = "_dd.p.tid";
// Approximates the container (`Value` or map entry) that holds each copied string, so refs to
// empty strings still count.
const STRING_COPY_OVERHEAD_BYTES: usize = std::mem::size_of::<Value>();

/// Bounds the bytes copied out of idx string tables, the payload-level values copied into every
/// chunk, and the values built from non-string scalars, across all idx tracer payloads in one
/// request.
///
/// A single string table entry can be referenced any number of times, and a scalar that takes a few
/// bytes on the wire can become a much longer string or a whole `Value` slot, so the decoded events
/// can be far larger than the request body that the decompression limit already bounds.
pub(super) struct ExpansionBudget {
    limit: usize,
    used: Cell<usize>,
}

impl ExpansionBudget {
    pub(super) const fn new(limit: usize) -> Self {
        Self {
            limit,
            used: Cell::new(0),
        }
    }

    fn charge(&self, bytes: usize) -> Result<(), IdxConversionError> {
        self.used
            .get()
            .checked_add(bytes)
            .filter(|&used| used <= self.limit)
            .map(|used| self.used.set(used))
            .ok_or(IdxConversionError::ExpansionLimitExceeded { limit: self.limit })
    }

    /// Charges one copy of the Agent fields written onto every trace event.
    ///
    /// `copies` is the number of events converted from indexed payloads. Events from legacy payloads
    /// are not charged, because their count is already bounded by the encoded body size.
    pub(super) fn charge_agent_metadata(
        &self,
        copies: usize,
        hostname: &str,
        env: &str,
        agent_version: &str,
        tags: &ObjectMap,
    ) -> Result<(), IdxConversionError> {
        let per_copy = [hostname, env, agent_version]
            .into_iter()
            .map(|value| value.len().saturating_add(STRING_COPY_OVERHEAD_BYTES))
            .chain(tags.iter().map(|(key, value)| {
                key.len()
                    .saturating_add(value.as_bytes().map_or(0, Bytes::len))
                    .saturating_add(STRING_COPY_OVERHEAD_BYTES)
            }))
            .fold(0, usize::saturating_add);
        self.charge(
            per_copy
                .checked_mul(copies)
                .ok_or(IdxConversionError::ExpansionLimitExceeded { limit: self.limit })?,
        )
    }
}

#[derive(Debug)]
pub(super) enum IdxConversionError {
    InvalidStringRef(InvalidStringRef),
    ExpansionLimitExceeded { limit: usize },
}

impl From<InvalidStringRef> for IdxConversionError {
    fn from(error: InvalidStringRef) -> Self {
        Self::InvalidStringRef(error)
    }
}

impl fmt::Display for IdxConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStringRef(error) => error.fmt(formatter),
            Self::ExpansionLimitExceeded { limit } => write!(
                formatter,
                "idx trace payload expands beyond the limit of {limit} bytes"
            ),
        }
    }
}

impl std::error::Error for IdxConversionError {}

/// A string table whose lookups are charged to the request's [`ExpansionBudget`].
///
/// Every lookup is charged before the caller copies the string, so callers must only look up
/// strings they are about to copy.
#[derive(Clone, Copy)]
struct PayloadStrings<'a> {
    table: StringTable<'a>,
    budget: &'a ExpansionBudget,
}

impl<'a> PayloadStrings<'a> {
    fn get(&self, index: u32) -> Result<&'a str, IdxConversionError> {
        let value = self.table.get(index)?;
        self.budget
            .charge(value.len().saturating_add(STRING_COPY_OVERHEAD_BYTES))?;
        Ok(value)
    }

    /// Charges a value that the conversion builds from a non-string scalar, such as the fixed-point
    /// spelling of a double or a hex-encoded id.
    ///
    /// These values are not references into the string table, but their stored form can be far
    /// larger than their encoding (`5e-324` is an 8-byte double and a 326-byte tag), so they count
    /// like copied strings. `heap_bytes` is what the value allocates beyond its `Value` slot.
    fn charge_built(&self, heap_bytes: usize) -> Result<(), IdxConversionError> {
        self.budget
            .charge(heap_bytes.saturating_add(STRING_COPY_OVERHEAD_BYTES))
    }

    /// Looks up a string that is copied into a JSON value and then into its escaped text.
    fn get_for_json(&self, index: u32) -> Result<&'a str, IdxConversionError> {
        let value = self.table.get(index)?;
        self.budget.charge(
            value
                .len()
                .saturating_add(json_string_len(value))
                .saturating_add(STRING_COPY_OVERHEAD_BYTES),
        )?;
        Ok(value)
    }
}

/// The length of `value` serialized as a JSON string by `serde_json`, including its quotes.
fn json_string_len(value: &str) -> usize {
    value
        .bytes()
        .map(|byte| match byte {
            b'"' | b'\\' | b'\x08' | b'\x0c' | b'\n' | b'\r' | b'\t' => 2,
            0x00..=0x1f => 6,
            _ => 1,
        })
        .fold(2, usize::saturating_add)
}

/// The length of `String::from_utf8_lossy(bytes)`, which replaces each invalid sequence with the
/// three-byte U+FFFD.
fn lossy_utf8_len(bytes: &[u8]) -> usize {
    bytes
        .utf8_chunks()
        .map(|chunk| {
            chunk.valid().len()
                + if chunk.invalid().is_empty() {
                    0
                } else {
                    char::REPLACEMENT_CHARACTER.len_utf8()
                }
        })
        .fold(0, usize::saturating_add)
}

struct IdxTracerFields<'a> {
    tags: &'a ObjectMap,
    container_id: &'a str,
    language_name: &'a str,
    language_version: &'a str,
    tracer_version: &'a str,
    runtime_id: &'a str,
    app_version: &'a str,
    /// The bytes copied into every chunk's event from the fields above.
    per_chunk_copy_bytes: usize,
}

pub(super) fn convert_idx_tracer_payload(
    payload: ddtrace_proto::idx::TracerPayload,
    budget: &ExpansionBudget,
) -> Result<Vec<TraceEvent>, IdxConversionError> {
    let strings = PayloadStrings {
        table: StringTable::new(&payload.strings),
        budget,
    };
    let payload_tags = attributes_as_strings(&strings, &payload.attributes)?;
    let container_id = strings.get(payload.container_id_ref)?;
    let language_name = strings.get(payload.language_name_ref)?;
    let language_version = strings.get(payload.language_version_ref)?;
    let tracer_version = strings.get(payload.tracer_version_ref)?;
    let runtime_id = strings.get(payload.runtime_id_ref)?;
    let app_version = strings.get(payload.app_version_ref)?;
    let per_chunk_copy_bytes = payload_tags
        .iter()
        .map(|(key, value)| {
            key.len() + value.as_bytes().map_or(0, Bytes::len) + STRING_COPY_OVERHEAD_BYTES
        })
        .chain(
            [
                container_id,
                language_name,
                language_version,
                tracer_version,
                runtime_id,
                app_version,
            ]
            .map(|value| value.len() + STRING_COPY_OVERHEAD_BYTES),
        )
        .fold(0, usize::saturating_add);
    let fields = IdxTracerFields {
        tags: &payload_tags,
        container_id,
        language_name,
        language_version,
        tracer_version,
        runtime_id,
        app_version,
        per_chunk_copy_bytes,
    };

    payload
        .chunks
        .into_iter()
        .map(|chunk| convert_idx_chunk(&strings, &fields, chunk))
        .collect()
}

fn convert_idx_chunk(
    strings: &PayloadStrings<'_>,
    fields: &IdxTracerFields<'_>,
    chunk: ddtrace_proto::idx::TraceChunk,
) -> Result<TraceEvent, IdxConversionError> {
    strings.budget.charge(fields.per_chunk_copy_bytes)?;
    let (trace_id_high, trace_id_low) = split_trace_id(&chunk.trace_id);
    let trace_id_high_tag = (trace_id_high != 0).then(|| format!("{trace_id_high:016x}"));

    let mut tags = attributes_as_strings(strings, &chunk.attributes)?;
    if chunk.sampling_mechanism != 0 && !tags.contains_key(SAMPLING_MECHANISM_TAG) {
        tags.insert(
            SAMPLING_MECHANISM_TAG.into(),
            Value::from(format_sampling_mechanism(chunk.sampling_mechanism)),
        );
    }
    tags.extend(fields.tags.clone());

    let mut trace_event = new_trace_event();
    trace_event.insert(event_path!("priority"), i64::from(chunk.priority));
    trace_event.insert(
        event_path!("origin"),
        strings.get(chunk.origin_ref)?.to_owned(),
    );
    trace_event.insert(event_path!("dropped"), chunk.dropped_trace);
    trace_event.insert(event_path!("tags"), Value::from(tags));
    let root_span_id = chunk
        .spans
        .iter()
        .find(|span| span.parent_id == 0)
        .or_else(|| chunk.spans.first())
        .map(|span| span.span_id);
    trace_event.insert(
        event_path!("spans"),
        chunk
            .spans
            .into_iter()
            .map(|span| {
                let trace_id_high_tag = trace_id_high_tag
                    .as_deref()
                    .filter(|_| root_span_id == Some(span.span_id));
                convert_idx_span(strings, span, trace_id_low, trace_id_high_tag).map(Value::from)
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    trace_event.insert(event_path!("container_id"), fields.container_id.to_owned());
    trace_event.insert(
        event_path!("language_name"),
        fields.language_name.to_owned(),
    );
    trace_event.insert(
        event_path!("language_version"),
        fields.language_version.to_owned(),
    );
    trace_event.insert(
        event_path!("tracer_version"),
        fields.tracer_version.to_owned(),
    );
    trace_event.insert(event_path!("runtime_id"), fields.runtime_id.to_owned());
    trace_event.insert(event_path!("app_version"), fields.app_version.to_owned());
    Ok(trace_event)
}

fn convert_idx_span(
    strings: &PayloadStrings<'_>,
    span: ddtrace_proto::idx::Span,
    trace_id_low: u64,
    trace_id_high_tag: Option<&str>,
) -> Result<ObjectMap, IdxConversionError> {
    let mut attributes = SpanAttributes::from_attributes(strings, span.attributes)?;
    if let Some(trace_id_high_tag) = trace_id_high_tag
        && !attributes.meta.contains_key(TRACE_ID_HIGH_TAG)
    {
        attributes.meta.insert(
            TRACE_ID_HIGH_TAG.into(),
            Value::from(trace_id_high_tag.to_owned()),
        );
    }
    for (key, value_ref) in [
        ("env", span.env_ref),
        ("version", span.version_ref),
        ("component", span.component_ref),
    ] {
        attributes.insert_promoted(key, strings.get(value_ref)?);
    }
    if let Some(kind) = promoted_span_kind(span.kind) {
        attributes.insert_promoted("span.kind", kind);
    }

    let span_links = span
        .links
        .into_iter()
        .map(|link| convert_idx_link(strings, &link).map(Value::from))
        .collect::<Result<Vec<_>, _>>()?;
    let span_events = span
        .events
        .into_iter()
        .map(|event| convert_idx_event(strings, &event).map(Value::from))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(value!({
        service: (strings.get(span.service_ref)?),
        name: (strings.get(span.name_ref)?),
        resource: (strings.get(span.resource_ref)?),
        trace_id: (u64_as_i64(trace_id_low)),
        span_id: (u64_as_i64(span.span_id)),
        parent_id: (u64_as_i64(span.parent_id)),
        start: (Utc.timestamp_nanos(u64_as_i64(span.start))),
        duration: (u64_as_i64(span.duration)),
        error: (i64::from(span.error)),
        meta: (attributes.meta),
        metrics: (attributes.metrics),
        "type": (strings.get(span.type_ref)?),
        meta_struct: (attributes.meta_struct),
        span_links: span_links,
        span_events: span_events,
    })
    .into_object()
    .expect("value! produces an object"))
}

fn convert_idx_link(
    strings: &PayloadStrings<'_>,
    link: &ddtrace_proto::idx::SpanLink,
) -> Result<ObjectMap, IdxConversionError> {
    let (trace_id_high, trace_id_low) = split_trace_id(&link.trace_id);
    Ok(ObjectMap::from([
        ("trace_id".into(), hex_id(strings, trace_id_low)?),
        ("trace_id_high".into(), hex_id(strings, trace_id_high)?),
        ("span_id".into(), hex_id(strings, link.span_id)?),
        (
            "attributes".into(),
            Value::from(attributes_as_strings(strings, &link.attributes)?),
        ),
        (
            "tracestate".into(),
            Value::from(strings.get(link.tracestate_ref)?),
        ),
        ("flags".into(), Value::from(i64::from(link.flags))),
    ]))
}

/// Encodes a link id as hex. Even an empty link emits three of these strings.
fn hex_id(strings: &PayloadStrings<'_>, id: u64) -> Result<Value, IdxConversionError> {
    let text = encode_u64_id_hex(id);
    strings.charge_built(text.len())?;
    Ok(Value::from(text))
}

fn convert_idx_event(
    strings: &PayloadStrings<'_>,
    event: &ddtrace_proto::idx::SpanEvent,
) -> Result<ObjectMap, IdxConversionError> {
    Ok(ObjectMap::from([
        (
            "time_unix_nano".into(),
            Value::from(Utc.timestamp_nanos(u64_as_i64(event.time))),
        ),
        ("name".into(), Value::from(strings.get(event.name_ref)?)),
        (
            "attributes".into(),
            Value::from(event_attributes(strings, &event.attributes)?),
        ),
    ]))
}

struct SpanAttributes {
    meta: ObjectMap,
    metrics: ObjectMap,
    meta_struct: ObjectMap,
}

impl SpanAttributes {
    fn from_attributes(
        strings: &PayloadStrings<'_>,
        attributes: BTreeMap<u32, AnyValue>,
    ) -> Result<Self, IdxConversionError> {
        let mut split = Self {
            meta: ObjectMap::new(),
            metrics: ObjectMap::new(),
            meta_struct: ObjectMap::new(),
        };
        for (key_ref, value) in attributes {
            let key = strings.get(key_ref)?.to_owned();
            match value.value {
                Some(IdxValue::StringValueRef(index)) => {
                    split
                        .meta
                        .insert(key.into(), Value::from(strings.get(index)?));
                }
                Some(IdxValue::BoolValue(bool_value)) => {
                    split.meta.insert(
                        key.into(),
                        Value::from(if bool_value { "true" } else { "false" }),
                    );
                }
                Some(IdxValue::IntValue(int_value)) => {
                    split.metrics.insert(key.into(), metric_from_int(int_value));
                }
                Some(IdxValue::DoubleValue(double_value)) => {
                    split
                        .metrics
                        .insert(key.into(), metric_from_float(double_value));
                }
                Some(IdxValue::BytesValue(bytes)) => {
                    split
                        .meta_struct
                        .insert(key.into(), Value::from(Bytes::from(bytes)));
                }
                Some(value @ (IdxValue::ArrayValue(_) | IdxValue::KeyValueList(_))) => {
                    split.meta.insert(
                        key.into(),
                        Value::from(json_text(&any_value_json(
                            strings,
                            &AnyValue { value: Some(value) },
                        )?)),
                    );
                }
                None => {}
            }
        }
        Ok(split)
    }

    fn insert_promoted(&mut self, key: &str, value: &str) {
        if !value.is_empty()
            && !self.meta.contains_key(key)
            && !self.metrics.contains_key(key)
            && !self.meta_struct.contains_key(key)
        {
            self.meta.insert(key.into(), Value::from(value));
        }
    }
}

fn attributes_as_strings(
    strings: &PayloadStrings<'_>,
    attributes: &BTreeMap<u32, AnyValue>,
) -> Result<ObjectMap, IdxConversionError> {
    attributes
        .iter()
        .map(|(key_ref, value)| {
            Ok((
                strings.get(*key_ref)?.to_owned().into(),
                Value::from(any_value_as_string(strings, value)?),
            ))
        })
        .collect()
}

fn event_attributes(
    strings: &PayloadStrings<'_>,
    attributes: &BTreeMap<u32, AnyValue>,
) -> Result<ObjectMap, IdxConversionError> {
    attributes
        .iter()
        .map(|(key_ref, value)| {
            Ok((
                strings.get(*key_ref)?.to_owned().into(),
                event_attribute_value(strings, value)?,
            ))
        })
        .collect()
}

fn event_attribute_value(
    strings: &PayloadStrings<'_>,
    value: &AnyValue,
) -> Result<Value, IdxConversionError> {
    // A two-byte `AnyValue` in an array still becomes a whole `Value` slot.
    let slot = |scalar: Value| strings.charge_built(0).map(|()| scalar);
    match &value.value {
        None => slot(Value::Null),
        Some(IdxValue::StringValueRef(index)) => Ok(Value::from(strings.get(*index)?)),
        Some(IdxValue::BoolValue(bool_value)) => slot(Value::from(*bool_value)),
        Some(IdxValue::IntValue(int_value)) => slot(Value::from(*int_value)),
        Some(IdxValue::DoubleValue(double_value)) => slot(metric_from_float(*double_value)),
        Some(IdxValue::ArrayValue(array)) if array.values.iter().all(is_scalar) => {
            Ok(Value::Array(
                array
                    .values
                    .iter()
                    .map(|item| event_attribute_value(strings, item))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }
        Some(IdxValue::BytesValue(_) | IdxValue::ArrayValue(_) | IdxValue::KeyValueList(_)) => {
            Ok(Value::from(json_text(&any_value_json(strings, value)?)))
        }
    }
}

const fn is_scalar(value: &AnyValue) -> bool {
    matches!(
        value.value,
        None | Some(
            IdxValue::StringValueRef(_)
                | IdxValue::BoolValue(_)
                | IdxValue::IntValue(_)
                | IdxValue::DoubleValue(_)
        )
    )
}

fn any_value_as_string(
    strings: &PayloadStrings<'_>,
    value: &AnyValue,
) -> Result<String, IdxConversionError> {
    let text = match &value.value {
        None => String::new(),
        Some(IdxValue::StringValueRef(index)) => return Ok(strings.get(*index)?.to_owned()),
        Some(IdxValue::BoolValue(bool_value)) => {
            if *bool_value { "true" } else { "false" }.to_owned()
        }
        Some(IdxValue::IntValue(int_value)) => int_value.to_string(),
        Some(IdxValue::DoubleValue(double_value)) => format_f64(*double_value),
        Some(IdxValue::BytesValue(bytes)) => {
            strings
                .budget
                .charge(lossy_utf8_len(bytes).saturating_add(STRING_COPY_OVERHEAD_BYTES))?;
            return Ok(String::from_utf8_lossy(bytes).into_owned());
        }
        Some(IdxValue::ArrayValue(_) | IdxValue::KeyValueList(_)) => {
            return Ok(json_text(&any_value_json(strings, value)?));
        }
    };
    strings.charge_built(text.len())?;
    Ok(text)
}

fn any_value_json(
    strings: &PayloadStrings<'_>,
    value: &AnyValue,
) -> Result<JsonValue, IdxConversionError> {
    match &value.value {
        Some(IdxValue::StringValueRef(index)) => {
            Ok(JsonValue::String(strings.get_for_json(*index)?.to_owned()))
        }
        Some(IdxValue::BoolValue(bool_value)) => Ok(JsonValue::Bool(*bool_value)),
        Some(IdxValue::IntValue(int_value)) => Ok(JsonValue::Number((*int_value).into())),
        Some(IdxValue::DoubleValue(double_value)) => Ok(json_f64(*double_value)),
        Some(IdxValue::BytesValue(bytes)) => {
            strings
                .budget
                .charge(bytes.len().div_ceil(3).saturating_mul(4 * 2))?;
            Ok(JsonValue::String(BASE64_STANDARD.encode(bytes)))
        }
        Some(IdxValue::ArrayValue(array)) => Ok(JsonValue::Array(
            array
                .values
                .iter()
                .map(|item| any_value_json(strings, item))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Some(IdxValue::KeyValueList(list)) => {
            let mut entries = Vec::with_capacity(list.key_values.len());
            for entry in &list.key_values {
                let key = strings.get_for_json(entry.key)?.to_owned();
                let json = entry
                    .value
                    .as_ref()
                    .map(|value| any_value_json(strings, value))
                    .transpose()?
                    .unwrap_or(JsonValue::Null);
                entries.push((key, json));
            }
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut object = JsonMap::new();
            for (key, json) in entries {
                object.insert(key, json);
            }
            Ok(JsonValue::Object(object))
        }
        None => Ok(JsonValue::Null),
    }
}

/// A double nested inside compound JSON.
///
/// Arrays and key-value lists become JSON text through `serde_json`. The Agent's
/// `AnyValue.AsString` does not: it joins those values with commas. Non-finite
/// doubles have no JSON number, so they become `null`.
fn json_f64(value: f64) -> JsonValue {
    serde_json::Number::from_f64(value).map_or(JsonValue::Null, JsonValue::Number)
}

fn json_text(value: &JsonValue) -> String {
    serde_json::to_string(value).expect("serializing a serde_json::Value cannot fail")
}

/// Renders `value` the way the Agent's `AnyValue.AsString` does.
///
/// That accessor uses `strconv.FormatFloat(value, 'f', -1, 64)`. Non-finite values use
/// Go's spellings (`NaN`, `+Inf`, `-Inf`). A finite value goes through [`Decimal`]: the
/// shortest round-trip digits come from the same Ryu algorithm Go uses, so an exact tie
/// breaks to even, and the string is fixed point, including `-0`.
fn format_f64(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value.is_infinite() {
        if value.is_sign_positive() {
            "+Inf"
        } else {
            "-Inf"
        }
        .to_owned()
    } else {
        Decimal::try_from_f64(value, FloatPrecision::RoundTrip)
            .expect("a finite f64 is within Decimal's magnitude limits")
            .to_string()
    }
}

fn promoted_span_kind(kind: i32) -> Option<&'static str> {
    match SpanKind::try_from(kind) {
        Ok(SpanKind::Unspecified) | Err(_) => None,
        Ok(kind) => Some(span_kind_name(kind)),
    }
}

fn metric_from_float(value: f64) -> Value {
    NotNan::new(value).map_or(Value::Null, Value::Float)
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Legacy span metrics are f64, and the Datadog traces sink drops other metric types."
)]
fn metric_from_int(value: i64) -> Value {
    metric_from_float(value as f64)
}

#[allow(
    clippy::cast_possible_wrap,
    reason = "Preserve the existing numeric conversion semantics of this source. https://github.com/vectordotdev/vector/issues/23659"
)]
const fn u64_as_i64(value: u64) -> i64 {
    value as i64
}
