use std::{cell::Cell, collections::BTreeMap, fmt};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
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

const SAMPLING_MECHANISM_TAG: &str = "_dd.p.dm";
const TRACE_ID_HIGH_TAG: &str = "_dd.p.tid";
// Approximates the container (`Value` or map entry) that holds each copied string, so refs to
// empty strings still count.
const STRING_COPY_OVERHEAD_BYTES: usize = std::mem::size_of::<Value>();

/// Bounds the bytes copied out of idx string tables, and the payload-level values copied into
/// every chunk, across all idx tracer payloads in one request.
///
/// A single string table entry can be referenced any number of times, so the decoded events can
/// be far larger than the request body that the decompression limit already bounds.
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
    /// `copies` is the number of events the request will emit. The caller uses this only when the
    /// request contains indexed payloads, because those payloads can contain many more chunks than
    /// the encoded body size suggests.
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
        (
            "trace_id".into(),
            Value::from(encode_u64_id_hex(trace_id_low)),
        ),
        (
            "trace_id_high".into(),
            Value::from(encode_u64_id_hex(trace_id_high)),
        ),
        (
            "span_id".into(),
            Value::from(encode_u64_id_hex(link.span_id)),
        ),
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
    match &value.value {
        None => Ok(Value::Null),
        Some(IdxValue::StringValueRef(index)) => Ok(Value::from(strings.get(*index)?)),
        Some(IdxValue::BoolValue(bool_value)) => Ok(Value::from(*bool_value)),
        Some(IdxValue::IntValue(int_value)) => Ok(Value::from(*int_value)),
        Some(IdxValue::DoubleValue(double_value)) => Ok(metric_from_float(*double_value)),
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
    match &value.value {
        None => Ok(String::new()),
        Some(IdxValue::StringValueRef(index)) => Ok(strings.get(*index)?.to_owned()),
        Some(IdxValue::BoolValue(bool_value)) => {
            Ok(if *bool_value { "true" } else { "false" }.to_owned())
        }
        Some(IdxValue::IntValue(int_value)) => Ok(int_value.to_string()),
        Some(IdxValue::DoubleValue(double_value)) => Ok(format_f64(*double_value)),
        Some(IdxValue::BytesValue(bytes)) => {
            strings
                .budget
                .charge(lossy_utf8_len(bytes).saturating_add(STRING_COPY_OVERHEAD_BYTES))?;
            Ok(String::from_utf8_lossy(bytes).into_owned())
        }
        Some(IdxValue::ArrayValue(_) | IdxValue::KeyValueList(_)) => {
            Ok(json_text(&any_value_json(strings, value)?))
        }
    }
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
/// Go's spellings. Finite values delegate to [`format_f64_fixed`].
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
        format_f64_fixed(value)
    }
}

/// Renders a finite `value` like Go's `strconv.FormatFloat(value, 'f', -1, 64)`.
///
/// Digits come from `ryu`, a line-by-line port of the same Ryu algorithm Go's `strconv`
/// uses, so it breaks an exact tie between two equally short round-trip spellings to
/// even, the way Go does (`f64::to_string` instead rounds such a tie away from zero).
/// `ryu`'s own layout differs from Go's fixed-point `'f'`, though: it appends `.0` to
/// integers and switches to scientific notation outside roughly `1e-5..1e16`. This
/// re-parses its digits and expands them into Go's always-fixed layout instead of using
/// that string directly.
fn format_f64_fixed(value: f64) -> String {
    debug_assert!(value.is_finite(), "format_f64 handles non-finite values");
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }

    let mut buffer = ryu::Buffer::new();
    let shortest = buffer.format_finite(value);
    let (sign, shortest) = shortest
        .strip_prefix('-')
        .map_or(("", shortest), |rest| ("-", rest));

    // Split into significant digits and a decimal point position: `|value| = 0.<digits>
    // * 10^point`, so the point sits `point` places after the first digit (negative
    // means leading zeros after `"0."`), before trimming insignificant zeros below.
    let (mantissa, exponent) =
        shortest
            .split_once('e')
            .map_or((shortest, 0), |(mantissa, exponent)| {
                (
                    mantissa,
                    exponent
                        .parse()
                        .expect("ryu exponent is a valid base-10 integer"),
                )
            });
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits: Vec<u8> = int_part.bytes().chain(frac_part.bytes()).collect();
    let mut point = i32::try_from(int_part.len()).expect("mantissa fits in i32") + exponent;
    while digits.len() > 1 && digits.first() == Some(&b'0') {
        digits.remove(0);
        point -= 1;
    }
    while digits.len() > 1 && digits.last() == Some(&b'0') {
        digits.pop();
    }
    let length = i32::try_from(digits.len()).expect("digit count fits in i32");

    let body = if point <= 0 {
        let leading_zeros = usize::try_from(-point).expect("point <= 0 here");
        format!("0.{}{}", "0".repeat(leading_zeros), ascii_digits(&digits))
    } else if point >= length {
        let trailing_zeros = usize::try_from(point - length).expect("point >= length here");
        format!("{}{}", ascii_digits(&digits), "0".repeat(trailing_zeros))
    } else {
        let split = usize::try_from(point).expect("0 < point < length here");
        let (whole, fraction) = digits.split_at(split);
        format!("{}.{}", ascii_digits(whole), ascii_digits(fraction))
    };
    format!("{sign}{body}")
}

/// Interprets `bytes` as ASCII decimal digits, which is how [`format_f64_fixed`]
/// builds its working buffer.
fn ascii_digits(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("ryu digits are ASCII")
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

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::BTreeMap};

    use bytes::Bytes;
    use proptest::{
        num::f64::{NEGATIVE, NORMAL, POSITIVE, SUBNORMAL, ZERO},
        prelude::*,
    };
    use similar_asserts::assert_eq;
    use vrl::value;

    use super::{
        super::{super::ddtrace_proto, handler::convert_dd_tracer_payload},
        ExpansionBudget, IdxConversionError, convert_idx_tracer_payload, format_f64_fixed,
        json_string_len, lossy_utf8_len,
    };
    use crate::event::{ObjectMap, TraceEvent, Value};
    use ddtrace_proto::idx::{
        AnyValue, ArrayValue, KeyValue, KeyValueList, SpanKind, any_value::Value as IdxValue,
        join_trace_id,
    };

    struct Strings(RefCell<Vec<String>>);

    impl Strings {
        fn new() -> Self {
            Self(RefCell::new(vec![String::new()]))
        }

        fn from_vec(strings: Vec<String>) -> Self {
            Self(RefCell::new(strings))
        }

        fn intern(&self, value: &str) -> u32 {
            let mut strings = self.0.borrow_mut();
            strings
                .iter()
                .position(|existing| existing == value)
                .map_or_else(
                    || {
                        let index = u32::try_from(strings.len()).unwrap();
                        strings.push(value.to_owned());
                        index
                    },
                    |index| u32::try_from(index).unwrap(),
                )
        }

        fn string(&self, value: &str) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::StringValueRef(self.intern(value))),
            }
        }

        fn boolean(value: bool) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::BoolValue(value)),
            }
        }

        fn int(value: i64) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::IntValue(value)),
            }
        }

        fn double(value: f64) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::DoubleValue(value)),
            }
        }

        fn bytes(value: &[u8]) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::BytesValue(value.to_vec())),
            }
        }

        fn array(values: Vec<AnyValue>) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::ArrayValue(ArrayValue { values })),
            }
        }

        fn list(&self, entries: Vec<(&str, AnyValue)>) -> AnyValue {
            AnyValue {
                value: Some(IdxValue::KeyValueList(KeyValueList {
                    key_values: entries
                        .into_iter()
                        .map(|(key, value)| KeyValue {
                            key: self.intern(key),
                            value: Some(value),
                        })
                        .collect(),
                })),
            }
        }

        fn map(&self, entries: Vec<(&str, AnyValue)>) -> BTreeMap<u32, AnyValue> {
            entries
                .into_iter()
                .map(|(key, value)| (self.intern(key), value))
                .collect()
        }

        fn into_payload(
            self,
            chunks: Vec<ddtrace_proto::idx::TraceChunk>,
        ) -> ddtrace_proto::idx::TracerPayload {
            ddtrace_proto::idx::TracerPayload {
                strings: self.0.into_inner(),
                chunks,
                ..Default::default()
            }
        }
    }

    fn base_span(strings: &Strings) -> ddtrace_proto::idx::Span {
        ddtrace_proto::idx::Span {
            service_ref: strings.intern("svc"),
            name_ref: strings.intern("op"),
            resource_ref: strings.intern("res"),
            type_ref: strings.intern("web"),
            span_id: 7,
            parent_id: 8,
            start: 1_000,
            duration: 50,
            ..Default::default()
        }
    }

    fn chunk(
        trace_id_high: u64,
        trace_id_low: u64,
        spans: Vec<ddtrace_proto::idx::Span>,
    ) -> ddtrace_proto::idx::TraceChunk {
        ddtrace_proto::idx::TraceChunk {
            trace_id: join_trace_id(trace_id_high, trace_id_low).to_vec(),
            spans,
            ..Default::default()
        }
    }

    fn convert(payload: ddtrace_proto::idx::TracerPayload) -> Vec<TraceEvent> {
        convert_idx_tracer_payload(payload, &ExpansionBudget::new(usize::MAX)).unwrap()
    }

    fn convert_err(payload: ddtrace_proto::idx::TracerPayload) -> IdxConversionError {
        convert_idx_tracer_payload(payload, &ExpansionBudget::new(usize::MAX)).unwrap_err()
    }

    fn span_at(event: &TraceEvent, index: usize) -> &ObjectMap {
        event.as_map().get("spans").unwrap().as_array().unwrap()[index]
            .as_object()
            .unwrap()
    }

    fn object(value: &Value) -> &ObjectMap {
        value.as_object().unwrap()
    }

    #[test]
    fn span_attributes_cover_every_any_value_type() {
        let strings = Strings::new();
        let attributes = strings.map(vec![
            ("str-key", strings.string("hello")),
            ("bool-key", Strings::boolean(true)),
            ("int-key", Strings::int(2)),
            ("double-key", Strings::double(1.5)),
            ("nan-key", Strings::double(f64::NAN)),
            ("bytes-key", Strings::bytes(b"hi")),
            (
                "array-key",
                Strings::array(vec![
                    strings.string("item"),
                    Strings::boolean(true),
                    Strings::int(1),
                ]),
            ),
            (
                "map-key",
                strings.list(vec![("b", strings.string("z")), ("a", Strings::int(1))]),
            ),
        ]);
        let mut span = base_span(&strings);
        span.attributes = attributes;
        let event = &convert(strings.into_payload(vec![chunk(0, 123, vec![span])]))[0];
        let span = span_at(event, 0);

        assert_eq!(
            span.get("meta").unwrap().clone(),
            value!({
                "str-key": "hello",
                "bool-key": "true",
                "array-key": "[\"item\",true,1]",
                "map-key": "{\"a\":1,\"b\":\"z\"}",
            })
        );
        assert_eq!(
            span.get("metrics").unwrap().clone(),
            value!({
                "int-key": 2.0,
                "double-key": 1.5,
                "nan-key": null,
            })
        );
        assert_eq!(
            object(span.get("meta_struct").unwrap()),
            &ObjectMap::from([("bytes-key".into(), Value::from(Bytes::from_static(b"hi")),)])
        );
        assert!(!object(span.get("meta").unwrap()).contains_key("span.kind"));
    }

    #[test]
    fn span_events_keep_scalars_and_json_encode_compound_values() {
        let strings = Strings::new();
        let attributes = strings.map(vec![
            ("str", strings.string("hello")),
            ("flag", Strings::boolean(true)),
            ("count", Strings::int(4)),
            ("ratio", Strings::double(0.5)),
            ("missing", Strings::double(f64::NAN)),
            ("blob", Strings::bytes(b"hi")),
            (
                "scalars",
                Strings::array(vec![strings.string("x"), Strings::int(3)]),
            ),
            (
                "nested",
                Strings::array(vec![Strings::array(vec![Strings::int(1)])]),
            ),
            ("pairs", strings.list(vec![("k", Strings::boolean(true))])),
        ]);
        let mut span = base_span(&strings);
        span.events = vec![ddtrace_proto::idx::SpanEvent {
            time: 99,
            name_ref: strings.intern("exception"),
            attributes,
        }];
        let event = &convert(strings.into_payload(vec![chunk(0, 1, vec![span])]))[0];
        let attributes = object(
            object(
                &span_at(event, 0)
                    .get("span_events")
                    .unwrap()
                    .as_array()
                    .unwrap()[0],
            )
            .get("attributes")
            .unwrap(),
        );

        assert_eq!(attributes.get("str").unwrap(), &Value::from("hello"));
        assert_eq!(attributes.get("flag").unwrap(), &Value::from(true));
        assert_eq!(attributes.get("count").unwrap(), &Value::from(4));
        assert_eq!(attributes.get("blob").unwrap(), &Value::from("\"aGk=\""));
        assert_eq!(attributes.get("nested").unwrap(), &Value::from("[[1]]"));
        assert_eq!(
            attributes.get("pairs").unwrap(),
            &Value::from("{\"k\":true}")
        );
        assert_eq!(attributes.get("scalars").unwrap().clone(), value!(["x", 3]));
        assert!(matches!(attributes.get("ratio").unwrap(), Value::Float(_)));
        assert_eq!(attributes.get("missing").unwrap(), &Value::Null);
    }

    #[test]
    fn link_attributes_are_strings_and_trace_ids_are_split() {
        let strings = Strings::new();
        let attributes = strings.map(vec![
            ("link", strings.string("yes")),
            ("flag", Strings::boolean(false)),
            ("blob", Strings::bytes(b"hi")),
            ("items", Strings::array(vec![strings.string("item")])),
        ]);
        let mut span = base_span(&strings);
        span.links = vec![ddtrace_proto::idx::SpanLink {
            trace_id: vec![0x0a, 0x0b],
            span_id: 0xdead,
            attributes,
            tracestate_ref: strings.intern("vendor=1"),
            flags: 3,
        }];
        let event = &convert(strings.into_payload(vec![chunk(0, 1, vec![span])]))[0];
        let link = object(
            &span_at(event, 0)
                .get("span_links")
                .unwrap()
                .as_array()
                .unwrap()[0],
        );

        assert_eq!(
            link.get("trace_id").unwrap(),
            &Value::from("0000000000000a0b")
        );
        assert_eq!(
            link.get("trace_id_high").unwrap(),
            &Value::from("0000000000000000")
        );
        assert_eq!(
            link.get("span_id").unwrap(),
            &Value::from("000000000000dead")
        );
        assert_eq!(link.get("tracestate").unwrap(), &Value::from("vendor=1"));
        assert_eq!(link.get("flags").unwrap(), &Value::from(3));
        assert_eq!(
            link.get("attributes").unwrap().clone(),
            value!({
                "link": "yes",
                "flag": "false",
                "blob": "hi",
                "items": "[\"item\"]",
            })
        );
    }

    #[test]
    fn trace_id_high_is_written_to_the_logical_root_only() {
        let strings = Strings::new();
        let mut child = base_span(&strings);
        child.span_id = 2;
        child.parent_id = 1;
        let mut root = base_span(&strings);
        root.span_id = 1;
        root.parent_id = 0;
        let events = convert(strings.into_payload(vec![chunk(0xab, 123, vec![child, root])]));
        let child_meta = object(span_at(&events[0], 0).get("meta").unwrap());
        let root_meta = object(span_at(&events[0], 1).get("meta").unwrap());

        assert_eq!(
            root_meta.get("_dd.p.tid").unwrap(),
            &Value::from("00000000000000ab")
        );
        assert!(!child_meta.contains_key("_dd.p.tid"));
        assert_eq!(
            span_at(&events[0], 0).get("trace_id").unwrap(),
            &Value::from(123)
        );
        assert_eq!(
            span_at(&events[0], 1).get("trace_id").unwrap(),
            &Value::from(123)
        );
    }

    #[test]
    fn trace_id_high_falls_back_to_the_first_span_when_no_span_is_a_root() {
        let strings = Strings::new();
        let mut first = base_span(&strings);
        first.span_id = 1;
        first.parent_id = 9;
        let mut second = base_span(&strings);
        second.span_id = 2;
        second.parent_id = 9;
        let events = convert(strings.into_payload(vec![chunk(0xab, 123, vec![first, second])]));
        let first_meta = object(span_at(&events[0], 0).get("meta").unwrap());
        let second_meta = object(span_at(&events[0], 1).get("meta").unwrap());

        assert_eq!(
            first_meta.get("_dd.p.tid").unwrap(),
            &Value::from("00000000000000ab")
        );
        assert!(!second_meta.contains_key("_dd.p.tid"));
    }

    #[test]
    fn trace_id_high_is_omitted_when_the_high_bits_are_zero() {
        let strings = Strings::new();
        let span = base_span(&strings);
        let events = convert(strings.into_payload(vec![chunk(0, 5, vec![span])]));
        let meta = object(span_at(&events[0], 0).get("meta").unwrap());
        assert!(!meta.contains_key("_dd.p.tid"));
    }

    #[test]
    fn existing_trace_id_high_tag_is_kept() {
        let strings = Strings::new();
        let mut span = base_span(&strings);
        span.attributes = strings.map(vec![("_dd.p.tid", strings.string("already"))]);
        let events = convert(strings.into_payload(vec![chunk(0xab, 5, vec![span])]));
        let meta = object(span_at(&events[0], 0).get("meta").unwrap());
        assert_eq!(meta.get("_dd.p.tid").unwrap(), &Value::from("already"));
    }

    #[test]
    fn sampling_mechanism_keeps_an_existing_chunk_tag() {
        let strings = Strings::new();
        let span = base_span(&strings);
        let mut trace = chunk(0, 1, vec![span]);
        trace.attributes = strings.map(vec![("_dd.p.dm", strings.string("keep"))]);
        trace.sampling_mechanism = 4;
        let events = convert(strings.into_payload(vec![trace]));
        let tags = object(events[0].as_map().get("tags").unwrap());
        assert_eq!(tags.get("_dd.p.dm").unwrap(), &Value::from("keep"));
    }

    #[test]
    fn chunk_attribute_doubles_match_the_agent_as_string() {
        // Spellings taken from Go's strconv.FormatFloat(v, 'f', -1, 64), which
        // AnyValue.AsString uses. Integral values have no decimal point, and
        // large values stay in fixed point rather than scientific notation.
        let cases = [
            ("hundred", 100.0, "100"),
            ("tiny", 1e-7, "0.0000001"),
            ("huge", 1e21, "1000000000000000000000"),
            ("neg-zero", -0.0, "-0"),
            ("pos-inf", f64::INFINITY, "+Inf"),
            ("neg-inf", f64::NEG_INFINITY, "-Inf"),
            ("nan", f64::NAN, "NaN"),
            ("negative", -1.5, "-1.5"),
            // f64::from_bits(0x42d85e4863b34e08) is exactly 107172881943864.125,
            // halfway between its two shortest round-trip spellings. Go's Ryu port
            // breaks the tie to even (.12); Rust's `Display` breaks it the other way
            // (.13), so this pins the Go spelling independent of `to_string`.
            (
                "tie",
                f64::from_bits(0x42d8_5e48_63b3_4e08),
                "107172881943864.12",
            ),
        ];
        let strings = Strings::new();
        let attributes = strings.map(
            cases
                .iter()
                .map(|(key, value, _)| (*key, Strings::double(*value)))
                .collect(),
        );
        let mut trace = chunk(0, 1, vec![base_span(&strings)]);
        trace.attributes = attributes;
        let events = convert(strings.into_payload(vec![trace]));
        let tags = object(events[0].as_map().get("tags").unwrap());
        for (key, _, expected) in cases {
            assert_eq!(tags.get(key).unwrap(), &Value::from(expected));
        }
    }

    #[test]
    fn sampling_mechanism_is_formatted_when_the_tag_is_absent() {
        let strings = Strings::new();
        let span = base_span(&strings);
        let mut trace = chunk(0, 1, vec![span]);
        trace.sampling_mechanism = 4;
        let present = convert(strings.into_payload(vec![trace]));
        assert_eq!(
            object(present[0].as_map().get("tags").unwrap())
                .get("_dd.p.dm")
                .unwrap(),
            &Value::from("-4")
        );

        let strings = Strings::new();
        let span = base_span(&strings);
        let absent = convert(strings.into_payload(vec![chunk(0, 1, vec![span])]));
        assert!(!object(absent[0].as_map().get("tags").unwrap()).contains_key("_dd.p.dm"));
    }

    #[test]
    fn payload_attribute_overrides_sampling_mechanism() {
        let strings = Strings::new();
        let span = base_span(&strings);
        let mut trace = chunk(0, 1, vec![span]);
        trace.sampling_mechanism = 4;
        let mut payload = strings.into_payload(vec![trace]);
        let strings = Strings::from_vec(payload.strings);
        payload.attributes = strings.map(vec![("_dd.p.dm", strings.string("from-payload"))]);
        payload.strings = strings.0.into_inner();
        let events = convert(payload);
        assert_eq!(
            object(events[0].as_map().get("tags").unwrap())
                .get("_dd.p.dm")
                .unwrap(),
            &Value::from("from-payload")
        );
    }

    #[test]
    fn span_kinds_are_written_unless_unspecified_or_already_set() {
        let strings = Strings::new();
        let mut spans = Vec::new();
        for kind in [
            SpanKind::Internal,
            SpanKind::Server,
            SpanKind::Client,
            SpanKind::Producer,
            SpanKind::Consumer,
            SpanKind::Unspecified,
        ] {
            let mut span = base_span(&strings);
            span.kind = i32::from(kind);
            spans.push(span);
        }
        let mut overridden = base_span(&strings);
        overridden.kind = i32::from(SpanKind::Server);
        overridden.attributes = strings.map(vec![("span.kind", strings.string("client"))]);
        overridden.env_ref = strings.intern("from-field");
        overridden
            .attributes
            .insert(strings.intern("env"), strings.string("from-attr"));
        overridden.component_ref = strings.intern("proxy");
        overridden
            .attributes
            .insert(strings.intern("component"), Strings::bytes(b"raw"));
        spans.push(overridden);

        let events = convert(strings.into_payload(vec![chunk(0, 1, spans)]));
        let kinds = ["internal", "server", "client", "producer", "consumer"];
        for (index, kind) in kinds.iter().enumerate() {
            assert_eq!(
                object(span_at(&events[0], index).get("meta").unwrap())
                    .get("span.kind")
                    .unwrap(),
                &Value::from(*kind)
            );
        }
        let unspecified = object(span_at(&events[0], 5).get("meta").unwrap());
        assert!(!unspecified.contains_key("span.kind"));

        let overridden = span_at(&events[0], 6);
        let meta = object(overridden.get("meta").unwrap());
        assert_eq!(meta.get("span.kind").unwrap(), &Value::from("client"));
        assert_eq!(meta.get("env").unwrap(), &Value::from("from-attr"));
        assert!(!meta.contains_key("component"));
        assert!(object(overridden.get("meta_struct").unwrap()).contains_key("component"));
    }

    #[test]
    fn bad_string_ref_fails() {
        let strings = Strings::new();
        let mut span = base_span(&strings);
        span.service_ref = 99;
        let error = convert_err(strings.into_payload(vec![chunk(0, 1, vec![span])]));
        assert!(matches!(
            error,
            IdxConversionError::InvalidStringRef(ref error) if error.index == 99
        ));
    }

    #[test]
    fn bad_link_string_ref_fails() {
        let strings = Strings::new();
        let mut span = base_span(&strings);
        span.links = vec![ddtrace_proto::idx::SpanLink {
            tracestate_ref: 99,
            ..Default::default()
        }];
        let error = convert_err(strings.into_payload(vec![chunk(0, 1, vec![span])]));
        assert!(matches!(
            error,
            IdxConversionError::InvalidStringRef(ref error) if error.index == 99
        ));
    }

    fn resource_fan_out_payload(
        resource_len: usize,
        spans: usize,
    ) -> ddtrace_proto::idx::TracerPayload {
        let strings = Strings::new();
        let resource_ref = strings.intern(&"r".repeat(resource_len));
        let spans = (0..spans)
            .map(|_| {
                let mut span = base_span(&strings);
                span.resource_ref = resource_ref;
                span
            })
            .collect();
        strings.into_payload(vec![chunk(0, 1, spans)])
    }

    #[test]
    fn empty_string_refs_still_count_against_the_budget() {
        let strings = Strings::new();
        let spans = (0..1024)
            .map(|_| ddtrace_proto::idx::Span::default())
            .collect();
        let error = convert_idx_tracer_payload(
            strings.into_payload(vec![chunk(0, 1, spans)]),
            &ExpansionBudget::new(4096),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            IdxConversionError::ExpansionLimitExceeded { .. }
        ));
    }

    #[test]
    fn repeated_string_refs_exceed_the_expansion_budget() {
        let payload = resource_fan_out_payload(1024, 64);
        let error =
            convert_idx_tracer_payload(payload, &ExpansionBudget::new(32 * 1024)).unwrap_err();
        assert!(matches!(
            error,
            IdxConversionError::ExpansionLimitExceeded { limit: 32_768 }
        ));

        let payload = resource_fan_out_payload(1024, 64);
        let events =
            convert_idx_tracer_payload(payload, &ExpansionBudget::new(128 * 1024)).unwrap();
        assert_eq!(events[0].as_map()["spans"].as_array().unwrap().len(), 64);
    }

    #[test]
    fn payload_tags_copied_into_each_chunk_count_against_the_budget() {
        let strings = Strings::new();
        let payload_tag = strings.string(&"t".repeat(1024));
        let chunks = (1..=64).map(|low| chunk(0, low, Vec::new())).collect();
        let mut payload = strings.into_payload(chunks);
        let tag_key = u32::try_from(payload.strings.len()).unwrap();
        payload.strings.push("payload-tag".to_owned());
        payload.attributes = BTreeMap::from([(tag_key, payload_tag)]);

        let error =
            convert_idx_tracer_payload(payload, &ExpansionBudget::new(32 * 1024)).unwrap_err();
        assert!(matches!(
            error,
            IdxConversionError::ExpansionLimitExceeded { .. }
        ));
    }

    proptest! {
        #[test]
        fn json_string_len_matches_serde_json(value: String) {
            prop_assert_eq!(
                json_string_len(&value),
                serde_json::to_string(&value).unwrap().len()
            );
        }

        #[test]
        fn lossy_utf8_len_matches_from_utf8_lossy(bytes: Vec<u8>) {
            prop_assert_eq!(
                lossy_utf8_len(&bytes),
                String::from_utf8_lossy(&bytes).len()
            );
        }

        /// Parsing the output must recover the input bits, so digit placement cannot
        /// drift; the literal cases in `chunk_attribute_doubles_match_the_agent_as_string`
        /// pin Go's tie-breaking, which this cannot observe on its own (`f64::parse`
        /// accepts both the tied spellings and ryu's own scientific-notation layout).
        #[test]
        fn format_f64_fixed_round_trips(value in POSITIVE | NEGATIVE | NORMAL | SUBNORMAL | ZERO) {
            let formatted = format_f64_fixed(value);
            prop_assert!(!formatted.contains('e'));
            prop_assert_eq!(formatted.parse::<f64>().unwrap().to_bits(), value.to_bits());
        }
    }

    #[test]
    fn lossy_utf8_replacement_counts_against_the_budget() {
        let strings = Strings::new();
        let mut trace = chunk(0, 1, Vec::new());
        trace.attributes = strings.map(vec![("k", Strings::bytes(&[0xff; 1024]))]);
        let error = convert_idx_tracer_payload(
            strings.into_payload(vec![trace]),
            &ExpansionBudget::new(2048),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            IdxConversionError::ExpansionLimitExceeded { .. }
        ));
    }

    #[test]
    fn json_escaping_counts_against_the_budget() {
        let strings = Strings::new();
        let mut span = base_span(&strings);
        span.attributes = strings.map(vec![(
            "k",
            Strings::array(vec![strings.string(&"\u{1}".repeat(1024))]),
        )]);
        let error = convert_idx_tracer_payload(
            strings.into_payload(vec![chunk(0, 1, vec![span])]),
            &ExpansionBudget::new(4096),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            IdxConversionError::ExpansionLimitExceeded { .. }
        ));
    }

    #[test]
    fn expansion_budget_is_shared_across_payloads() {
        let budget = ExpansionBudget::new(96 * 1024);
        convert_idx_tracer_payload(resource_fan_out_payload(1024, 64), &budget).unwrap();
        let error =
            convert_idx_tracer_payload(resource_fan_out_payload(1024, 64), &budget).unwrap_err();
        assert!(matches!(
            error,
            IdxConversionError::ExpansionLimitExceeded { .. }
        ));
    }

    #[test]
    fn negative_priority_is_preserved() {
        let strings = Strings::new();
        let mut trace = chunk(0, 1, vec![base_span(&strings)]);
        trace.priority = -128;
        let events = convert(strings.into_payload(vec![trace]));
        assert_eq!(
            events[0].as_map().get("priority").unwrap(),
            &Value::from(-128_i64)
        );
    }

    #[test]
    fn idx_payload_matches_the_legacy_conversion() {
        let legacy = representative_legacy_payload();
        let idx = representative_idx_payload();
        let mut legacy_events = convert_dd_tracer_payload(legacy);
        let idx_events = convert(idx);

        let second_meta = legacy_events[0]
            .get_mut(vrl::event_path!("spans"))
            .unwrap()
            .as_array_mut()
            .unwrap()[1]
            .as_object_mut()
            .unwrap()
            .get_mut("meta")
            .unwrap()
            .as_object_mut()
            .unwrap();
        second_meta.insert("span.kind".into(), Value::from("internal"));

        assert_eq!(legacy_events, idx_events);
    }

    fn representative_legacy_payload() -> ddtrace_proto::TracerPayload {
        use ddtrace_proto::attribute_any_value::AttributeAnyValueType;

        ddtrace_proto::TracerPayload {
            container_id: "container".to_owned(),
            language_name: "rust".to_owned(),
            language_version: "1.91".to_owned(),
            tracer_version: "1.2.3".to_owned(),
            runtime_id: "runtime".to_owned(),
            app_version: "app-1".to_owned(),
            env: "legacy-env".to_owned(),
            hostname: "legacy-host".to_owned(),
            tags: BTreeMap::from([
                ("payload".to_owned(), "tag".to_owned()),
                ("shared".to_owned(), "from-payload".to_owned()),
            ]),
            chunks: vec![ddtrace_proto::TraceChunk {
                priority: 2,
                origin: "lambda".to_owned(),
                dropped_trace: true,
                tags: BTreeMap::from([
                    ("chunk".to_owned(), "tag".to_owned()),
                    ("shared".to_owned(), "from-chunk".to_owned()),
                ]),
                spans: vec![
                    ddtrace_proto::Span {
                        service: "svc".to_owned(),
                        name: "op".to_owned(),
                        resource: "res".to_owned(),
                        trace_id: 0x1111,
                        span_id: 0x2222,
                        parent_id: 0,
                        start: 1_431_648_000_000_001,
                        duration: 50,
                        error: 1,
                        meta: BTreeMap::from([
                            ("foo".to_owned(), "bar".to_owned()),
                            ("env".to_owned(), "prod".to_owned()),
                            ("version".to_owned(), "v9".to_owned()),
                            ("component".to_owned(), "proxy".to_owned()),
                            ("span.kind".to_owned(), "server".to_owned()),
                        ]),
                        metrics: BTreeMap::from([("n".to_owned(), 1.5)]),
                        r#type: "web".to_owned(),
                        meta_struct: BTreeMap::from([("bin".to_owned(), b"hi".to_vec())]),
                        span_links: vec![ddtrace_proto::SpanLink {
                            trace_id: 7,
                            trace_id_high: 8,
                            span_id: 9,
                            attributes: BTreeMap::from([("link".to_owned(), "yes".to_owned())]),
                            tracestate: "vendor=1".to_owned(),
                            flags: 1,
                        }],
                        span_events: vec![ddtrace_proto::SpanEvent {
                            time_unix_nano: 99,
                            name: "exception".to_owned(),
                            attributes: BTreeMap::from([(
                                "exception.message".to_owned(),
                                ddtrace_proto::AttributeAnyValue {
                                    r#type: i32::from(AttributeAnyValueType::StringValue),
                                    string_value: "boom".to_owned(),
                                    ..Default::default()
                                },
                            )]),
                        }],
                    },
                    ddtrace_proto::Span {
                        service: "svc2".to_owned(),
                        name: "op2".to_owned(),
                        resource: "res2".to_owned(),
                        trace_id: 0x1111,
                        span_id: 0x3333,
                        parent_id: 0x2222,
                        start: 10,
                        duration: 1,
                        meta: BTreeMap::from([("only".to_owned(), "second".to_owned())]),
                        r#type: "db".to_owned(),
                        ..Default::default()
                    },
                ],
            }],
            ..Default::default()
        }
    }

    fn representative_idx_payload() -> ddtrace_proto::idx::TracerPayload {
        let strings = Strings::new();
        let mut root = base_span(&strings);
        root.service_ref = strings.intern("svc");
        root.name_ref = strings.intern("op");
        root.resource_ref = strings.intern("res");
        root.type_ref = strings.intern("web");
        root.span_id = 0x2222;
        root.parent_id = 0;
        root.start = 1_431_648_000_000_001;
        root.duration = 50;
        root.error = true;
        root.env_ref = strings.intern("prod");
        root.version_ref = strings.intern("v9");
        root.component_ref = strings.intern("proxy");
        root.kind = i32::from(SpanKind::Server);
        root.attributes = strings.map(vec![
            ("foo", strings.string("bar")),
            ("n", Strings::double(1.5)),
            ("bin", Strings::bytes(b"hi")),
        ]);
        root.links = vec![ddtrace_proto::idx::SpanLink {
            trace_id: join_trace_id(8, 7).to_vec(),
            span_id: 9,
            attributes: strings.map(vec![("link", strings.string("yes"))]),
            tracestate_ref: strings.intern("vendor=1"),
            flags: 1,
        }];
        root.events = vec![ddtrace_proto::idx::SpanEvent {
            time: 99,
            name_ref: strings.intern("exception"),
            attributes: strings.map(vec![("exception.message", strings.string("boom"))]),
        }];

        let mut child = base_span(&strings);
        child.service_ref = strings.intern("svc2");
        child.name_ref = strings.intern("op2");
        child.resource_ref = strings.intern("res2");
        child.type_ref = strings.intern("db");
        child.span_id = 0x3333;
        child.parent_id = 0x2222;
        child.start = 10;
        child.duration = 1;
        child.kind = i32::from(SpanKind::Internal);
        child.attributes = strings.map(vec![("only", strings.string("second"))]);

        let mut trace = chunk(0, 0x1111, vec![root, child]);
        trace.priority = 2;
        trace.origin_ref = strings.intern("lambda");
        trace.dropped_trace = true;
        trace.attributes = strings.map(vec![
            ("chunk", strings.string("tag")),
            ("shared", strings.string("from-chunk")),
        ]);

        let mut payload = strings.into_payload(vec![trace]);
        let strings = Strings::from_vec(payload.strings);
        payload.container_id_ref = strings.intern("container");
        payload.language_name_ref = strings.intern("rust");
        payload.language_version_ref = strings.intern("1.91");
        payload.tracer_version_ref = strings.intern("1.2.3");
        payload.runtime_id_ref = strings.intern("runtime");
        payload.app_version_ref = strings.intern("app-1");
        payload.env_ref = strings.intern("idx-env");
        payload.hostname_ref = strings.intern("idx-host");
        payload.attributes = strings.map(vec![
            ("payload", strings.string("tag")),
            ("shared", strings.string("from-payload")),
        ]);
        payload.container_debug = Some(ddtrace_proto::idx::ContainerDebug {
            was_buffered: true,
            ..Default::default()
        });
        payload.strings = strings.0.into_inner();
        payload
    }
}
