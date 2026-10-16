//! Parsers that rebuild protobuf `ResourceSpans` from a legacy OTLP layout.
//!
//! These recover protobuf values only. Fields a layout does not carry take their protobuf
//! defaults, and the shared decoding in `convert` applies the mapping rules.

use vector_core::event::typed_trace::{TraceConversionIssue, TraceConversionReporter};
use vrl::value::{ObjectMap, Value};

use crate::{
    proto::{
        common::v1::{
            AnyValue, ArrayValue, InstrumentationScope, KeyValue, KeyValueList,
            any_value::Value as PbValue,
        },
        resource::v1::Resource as PbResource,
        trace::v1::{
            ResourceSpans, ScopeSpans, Span as PbSpan, Status as PbStatus,
            span::{Event as PbEvent, Link as PbLink, SpanKind as PbSpanKind},
            status::StatusCode as PbStatusCode,
        },
    },
    spans::{
        ATTRIBUTES_KEY, DROPPED_ATTRIBUTES_COUNT_KEY, RESOURCE_KEY, SPAN_ID_KEY, TRACE_ID_KEY,
    },
};

/// A length no identifier has, so the shared span conversion rejects and counts it.
const INVALID_ID: &[u8] = &[0];

/// Flattened key the source writes on every event. It records when Vector received the span
/// and has no OTLP field, so dropping it is expected and not reported.
const INGEST_TIMESTAMP_KEY: &str = "ingest_timestamp";

/// One legacy object being read. [`Self::finish`] reports every key that no read took.
///
/// Parsers of nested values reborrow [`Self::reporter`] while the object is being read.
pub(super) struct ConsumeFields<'r, R> {
    map: ObjectMap,
    reporter: &'r mut R,
}

impl<'r, R: TraceConversionReporter> ConsumeFields<'r, R> {
    pub(super) fn wrap(map: ObjectMap, reporter: &'r mut R) -> Self {
        Self { map, reporter }
    }

    /// Removes `key`. `Null` reads as absent, and is still consumed.
    fn take(&mut self, key: &str) -> Option<Value> {
        match self.map.remove(key) {
            None | Some(Value::Null) => None,
            Some(value) => Some(value),
        }
    }

    /// Consumes `key` without reading it.
    fn skip(&mut self, key: &str) {
        self.map.remove(key);
    }

    /// Reports `field` as malformed and returns its default.
    fn malformed<T: Default>(&mut self, field: &'static str) -> T {
        self.reporter
            .report(TraceConversionIssue::MalformedField { field });
        T::default()
    }

    fn read_string(&mut self, key: &'static str) -> String {
        self.take(key).map_or_else(String::new, |value| {
            into_utf8(value).unwrap_or_else(|| self.malformed(key))
        })
    }

    fn read_object(&mut self, key: &'static str) -> Option<ObjectMap> {
        match self.take(key) {
            None => None,
            Some(Value::Object(object)) => Some(object),
            Some(_) => self.malformed(key),
        }
    }

    fn read_integer<T: TryFrom<i64> + Default>(&mut self, key: &'static str) -> T {
        match self.take(key) {
            None => T::default(),
            Some(Value::Integer(n)) => T::try_from(n).unwrap_or_else(|_| self.malformed(key)),
            Some(_) => self.malformed(key),
        }
    }

    fn read_u32(&mut self, key: &'static str) -> u32 {
        self.read_integer(key)
    }

    fn read_i32(&mut self, key: &'static str) -> i32 {
        self.read_integer(key)
    }

    /// Reads a `resourceSpans` `fixed64`, which the protobuf decoder stores as `u64 as i64`.
    fn read_fixed64(&mut self, key: &'static str) -> u64 {
        match self.take(key) {
            None => 0,
            Some(Value::Integer(n)) => i64::cast_unsigned(n),
            Some(_) => self.malformed(key),
        }
    }

    /// Reads a flattened timestamp. The source stores the wire `u64` nanoseconds as
    /// `Utc.timestamp_nanos(nanos as i64)`, so the cast back recovers values beyond `i64::MAX`.
    ///
    /// Returns `None` when absent or malformed.
    fn read_timestamp(&mut self, key: &'static str) -> Option<u64> {
        let value = self.take(key)?;
        let nanos = match value {
            Value::Timestamp(ts) => ts.timestamp_nanos_opt().map(i64::cast_unsigned),
            _ => None,
        };
        if nanos.is_none() {
            self.malformed::<()>(key);
        }
        nanos
    }

    /// Reads an enum that the protobuf decoder stores as its variant name.
    fn read_enum(&mut self, key: &'static str, from_name: fn(&str) -> Option<i32>) -> i32 {
        self.take(key).map_or(0, |value| {
            into_utf8(value)
                .and_then(|name| from_name(&name))
                .unwrap_or_else(|| self.malformed(key))
        })
    }

    pub(super) fn finish(self) {
        let Self { map, reporter } = self;
        for key in map.keys() {
            reporter.report(TraceConversionIssue::UnmappedField { key: key.as_str() });
        }
    }
}

/// Returns the entries of an optional list field. A present non-array value counts as one
/// malformed entry.
fn into_entries(value: Option<Value>) -> Vec<Value> {
    match value {
        None => Vec::new(),
        Some(Value::Array(values)) => values,
        Some(_) => vec![Value::Null],
    }
}

/// Parses each entry, reporting `malformed` for every entry that fails to parse.
///
/// Returns the parsed entries and the number dropped, for the caller's in-band count.
pub(super) fn parse_entries<T, R: TraceConversionReporter>(
    entries: impl IntoIterator<Item = Value>,
    malformed: TraceConversionIssue<'static>,
    reporter: &mut R,
    mut parse: impl FnMut(Value, &mut R) -> Option<T>,
) -> (Vec<T>, u32) {
    let mut dropped = 0_u32;
    let parsed = entries
        .into_iter()
        .filter_map(|entry| {
            let parsed = parse(entry, reporter);
            if parsed.is_none() {
                dropped = dropped.saturating_add(1);
                reporter.report(malformed);
            }
            parsed
        })
        .collect();
    (parsed, dropped)
}

/// Passes `parsed` through, counting a failed nested attribute value against `dropped`.
fn count_malformed_attribute<T>(
    parsed: Option<T>,
    dropped: &mut u32,
    reporter: &mut impl TraceConversionReporter,
) -> Option<T> {
    if parsed.is_none() {
        *dropped = dropped.saturating_add(1);
        reporter.report(TraceConversionIssue::MalformedAttribute);
    }
    parsed
}

impl ResourceSpans {
    pub(super) fn from_value(
        value: Value,
        reporter: &mut impl TraceConversionReporter,
    ) -> Option<Self> {
        let Value::Object(object) = value else {
            return None;
        };
        let mut fields = ConsumeFields::wrap(object, reporter);
        let resource = fields
            .read_object("resource")
            .map(|object| PbResource::from_object(object, fields.reporter));
        let scope_spans = into_entries(fields.take("scopeSpans"));
        let (scope_spans, _) = parse_entries(
            scope_spans,
            TraceConversionIssue::MalformedGrouping,
            fields.reporter,
            ScopeSpans::from_value,
        );
        let schema_url = fields.read_string("schemaUrl");
        fields.finish();
        Some(Self {
            resource,
            scope_spans,
            schema_url,
        })
    }

    pub(super) fn from_flattened_map(
        map: ObjectMap,
        reporter: &mut impl TraceConversionReporter,
    ) -> Self {
        let mut fields = ConsumeFields::wrap(map, reporter);
        fields.skip(INGEST_TIMESTAMP_KEY);
        let resource = PbResource::from_flattened(&mut fields);
        let span = PbSpan::from_flattened(&mut fields);
        fields.finish();
        Self {
            resource: Some(resource),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans: vec![span],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }
    }
}

impl ScopeSpans {
    fn from_value(value: Value, reporter: &mut impl TraceConversionReporter) -> Option<Self> {
        let Value::Object(object) = value else {
            return None;
        };
        let mut fields = ConsumeFields::wrap(object, reporter);
        let scope = fields
            .read_object("scope")
            .map(|object| InstrumentationScope::from_object(object, fields.reporter));
        // A non-object entry becomes an ID-less span so the shared conversion rejects and
        // counts it.
        let spans = into_entries(fields.take("spans"))
            .into_iter()
            .map(|span| PbSpan::from_value(span, fields.reporter).unwrap_or_default())
            .collect();
        let schema_url = fields.read_string("schemaUrl");
        fields.finish();
        Some(Self {
            scope,
            spans,
            schema_url,
        })
    }
}

impl PbResource {
    fn from_object(object: ObjectMap, reporter: &mut impl TraceConversionReporter) -> Self {
        let mut fields = ConsumeFields::wrap(object, reporter);
        let mut dropped_attributes_count = fields.read_u32("droppedAttributesCount");
        let attributes = fields.take("attributes");
        let attributes =
            array_to_key_values(attributes, &mut dropped_attributes_count, fields.reporter);
        fields.finish();
        Self {
            attributes,
            dropped_attributes_count,
        }
    }

    fn from_flattened(fields: &mut ConsumeFields<'_, impl TraceConversionReporter>) -> Self {
        let mut dropped_attributes_count = 0;
        let attributes = fields.take(RESOURCE_KEY);
        let attributes =
            object_to_key_values(attributes, &mut dropped_attributes_count, fields.reporter);
        Self {
            attributes,
            dropped_attributes_count,
        }
    }
}

impl InstrumentationScope {
    fn from_object(object: ObjectMap, reporter: &mut impl TraceConversionReporter) -> Self {
        let mut fields = ConsumeFields::wrap(object, reporter);
        let mut dropped_attributes_count = fields.read_u32("droppedAttributesCount");
        let attributes = fields.take("attributes");
        let attributes =
            array_to_key_values(attributes, &mut dropped_attributes_count, fields.reporter);
        let name = fields.read_string("name");
        let version = fields.read_string("version");
        fields.finish();
        Self {
            name,
            version,
            attributes,
            dropped_attributes_count,
        }
    }
}

impl PbSpan {
    fn from_value(value: Value, reporter: &mut impl TraceConversionReporter) -> Option<Self> {
        let Value::Object(object) = value else {
            return None;
        };
        let mut fields = ConsumeFields::wrap(object, reporter);
        let mut dropped_attributes_count = fields.read_u32("droppedAttributesCount");
        let attributes = fields.take("attributes");
        let attributes =
            array_to_key_values(attributes, &mut dropped_attributes_count, fields.reporter);
        let events = into_entries(fields.take("events"));
        let (events, malformed_events) = parse_entries(
            events,
            TraceConversionIssue::MalformedEvent,
            fields.reporter,
            PbEvent::from_value,
        );
        let links = into_entries(fields.take("links"))
            .into_iter()
            .map(|link| PbLink::from_value(link, fields.reporter).unwrap_or_default())
            .collect();
        let trace_id = raw_id(fields.take("traceId"));
        let span_id = raw_id(fields.take("spanId"));
        let trace_state = fields.read_string("traceState");
        let parent_span_id = raw_id(fields.take("parentSpanId"));
        let flags = fields.read_u32("flags");
        let name = fields.read_string("name");
        let kind = fields.read_enum("kind", |name| {
            PbSpanKind::from_str_name(name).map(i32::from)
        });
        let start_time_unix_nano = fields.read_fixed64("startTimeUnixNano");
        let end_time_unix_nano = fields.read_fixed64("endTimeUnixNano");
        let dropped_events_count = fields.read_u32("droppedEventsCount");
        let dropped_links_count = fields.read_u32("droppedLinksCount");
        let status = fields
            .read_object("status")
            .map(|object| PbStatus::from_object(object, fields.reporter));
        fields.finish();
        Some(Self {
            trace_id,
            span_id,
            trace_state,
            parent_span_id,
            flags,
            name,
            kind,
            start_time_unix_nano,
            end_time_unix_nano,
            attributes,
            dropped_attributes_count,
            events,
            dropped_events_count: dropped_events_count.saturating_add(malformed_events),
            links,
            dropped_links_count,
            status,
        })
    }

    fn from_flattened(fields: &mut ConsumeFields<'_, impl TraceConversionReporter>) -> Self {
        let start_time_unix_nano = fields.read_timestamp("start_time_unix_nano").unwrap_or(0);
        let end_time_unix_nano = fields.read_timestamp("end_time_unix_nano").unwrap_or(0);
        let mut dropped_attributes_count = fields.read_u32(DROPPED_ATTRIBUTES_COUNT_KEY);
        let attributes = fields.take(ATTRIBUTES_KEY);
        let attributes =
            object_to_key_values(attributes, &mut dropped_attributes_count, fields.reporter);
        let events = into_entries(fields.take("events"));
        let (events, malformed_events) = parse_entries(
            events,
            TraceConversionIssue::MalformedEvent,
            fields.reporter,
            PbEvent::from_flattened,
        );
        let links = into_entries(fields.take("links"))
            .into_iter()
            .map(|link| PbLink::from_flattened(link, fields.reporter).unwrap_or_default())
            .collect();
        let trace_id = hex_id(fields.take(TRACE_ID_KEY));
        let span_id = hex_id(fields.take(SPAN_ID_KEY));
        let trace_state = fields.read_string("trace_state");
        let parent_span_id = hex_id(fields.take("parent_span_id"));
        let name = fields.read_string("name");
        let kind = fields.read_i32("kind");
        let dropped_events_count = fields.read_u32("dropped_events_count");
        let dropped_links_count = fields.read_u32("dropped_links_count");
        let status = fields
            .read_object("status")
            .map(|object| PbStatus::from_flattened(object, fields.reporter));
        Self {
            trace_id,
            span_id,
            trace_state,
            parent_span_id,
            flags: 0,
            name,
            kind,
            start_time_unix_nano,
            end_time_unix_nano,
            attributes,
            dropped_attributes_count,
            events,
            dropped_events_count: dropped_events_count.saturating_add(malformed_events),
            links,
            dropped_links_count,
            status,
        }
    }
}

impl PbEvent {
    fn from_value(value: Value, reporter: &mut impl TraceConversionReporter) -> Option<Self> {
        let Value::Object(object) = value else {
            return None;
        };
        let mut fields = ConsumeFields::wrap(object, reporter);
        let mut dropped_attributes_count = fields.read_u32("droppedAttributesCount");
        let attributes = fields.take("attributes");
        let attributes =
            array_to_key_values(attributes, &mut dropped_attributes_count, fields.reporter);
        let event = Self {
            time_unix_nano: fields.read_fixed64("timeUnixNano"),
            name: fields.read_string("name"),
            attributes,
            dropped_attributes_count,
        };
        fields.finish();
        Some(event)
    }

    fn from_flattened(value: Value, reporter: &mut impl TraceConversionReporter) -> Option<Self> {
        let Value::Object(object) = value else {
            return None;
        };
        let mut fields = ConsumeFields::wrap(object, reporter);
        let mut dropped_attributes_count = fields.read_u32(DROPPED_ATTRIBUTES_COUNT_KEY);
        let attributes = fields.take(ATTRIBUTES_KEY);
        let attributes =
            object_to_key_values(attributes, &mut dropped_attributes_count, fields.reporter);
        let event = Self {
            time_unix_nano: fields.read_timestamp("time_unix_nano").unwrap_or(0),
            name: fields.read_string("name"),
            attributes,
            dropped_attributes_count,
        };
        fields.finish();
        Some(event)
    }
}

impl PbLink {
    fn from_value(value: Value, reporter: &mut impl TraceConversionReporter) -> Option<Self> {
        let Value::Object(object) = value else {
            return None;
        };
        let mut fields = ConsumeFields::wrap(object, reporter);
        let mut dropped_attributes_count = fields.read_u32("droppedAttributesCount");
        let attributes = fields.take("attributes");
        let attributes =
            array_to_key_values(attributes, &mut dropped_attributes_count, fields.reporter);
        let link = Self {
            trace_id: raw_id(fields.take("traceId")),
            span_id: raw_id(fields.take("spanId")),
            trace_state: fields.read_string("traceState"),
            attributes,
            dropped_attributes_count,
            flags: fields.read_u32("flags"),
        };
        fields.finish();
        Some(link)
    }

    fn from_flattened(value: Value, reporter: &mut impl TraceConversionReporter) -> Option<Self> {
        let Value::Object(object) = value else {
            return None;
        };
        let mut fields = ConsumeFields::wrap(object, reporter);
        let mut dropped_attributes_count = fields.read_u32(DROPPED_ATTRIBUTES_COUNT_KEY);
        let attributes = fields.take(ATTRIBUTES_KEY);
        let attributes =
            object_to_key_values(attributes, &mut dropped_attributes_count, fields.reporter);
        let link = Self {
            trace_id: hex_id(fields.take(TRACE_ID_KEY)),
            span_id: hex_id(fields.take(SPAN_ID_KEY)),
            trace_state: fields.read_string("trace_state"),
            attributes,
            dropped_attributes_count,
            flags: 0,
        };
        fields.finish();
        Some(link)
    }
}

impl PbStatus {
    fn from_object(object: ObjectMap, reporter: &mut impl TraceConversionReporter) -> Self {
        let mut fields = ConsumeFields::wrap(object, reporter);
        let status = Self {
            message: fields.read_string("message"),
            code: fields.read_enum("code", |name| {
                PbStatusCode::from_str_name(name).map(i32::from)
            }),
        };
        fields.finish();
        status
    }

    fn from_flattened(object: ObjectMap, reporter: &mut impl TraceConversionReporter) -> Self {
        let mut fields = ConsumeFields::wrap(object, reporter);
        let status = Self {
            message: fields.read_string("message"),
            code: fields.read_i32("code"),
        };
        fields.finish();
        status
    }
}

impl KeyValue {
    /// Parses a `resourceSpans` key-value entry. A present key that is not a string, or a
    /// malformed value, fails the entry as a whole.
    fn from_value(
        value: Value,
        dropped: &mut u32,
        reporter: &mut impl TraceConversionReporter,
    ) -> Option<Self> {
        let Value::Object(object) = value else {
            return None;
        };
        let mut fields = ConsumeFields::wrap(object, reporter);
        // `KeyValue.key` is a proto3 string, so its default is `""`. The `resourceSpans`
        // layout comes from VRL `proto_to_value`, which omits proto3 defaults, and an empty
        // key arrives with no `key` field. The wire `KeyValue` still carries `""`, and
        // `key_values_to_attr_map` keeps it, so recover that default here. Both inputs then
        // reach the shared conversion as the same protobuf.
        //
        // Remove this once events no longer come from that decoder. The direct protobuf path
        // already has the key, and an absent `key` on any other input is a malformed entry.
        let key = match fields.take("key") {
            None => String::new(),
            Some(value) => into_utf8(value)?,
        };
        let value = match fields.take("value") {
            None => None,
            Some(value) => Some(AnyValue::from_value(value, dropped, fields.reporter)?),
        };
        fields.finish();
        Some(Self { key, value })
    }
}

impl AnyValue {
    /// Parses a `resourceSpans` `AnyValue`, which the protobuf decoder stores as an object
    /// keyed by the set oneof member. Malformed nested elements are dropped and counted in
    /// `dropped`.
    fn from_value(
        value: Value,
        dropped: &mut u32,
        reporter: &mut impl TraceConversionReporter,
    ) -> Option<Self> {
        let Value::Object(object) = value else {
            return None;
        };
        let mut fields = ConsumeFields::wrap(object, reporter);
        let value = if let Some(v) = fields.take("stringValue") {
            Some(PbValue::StringValue(into_utf8(v)?))
        } else if let Some(v) = fields.take("boolValue") {
            let Value::Boolean(v) = v else {
                return None;
            };
            Some(PbValue::BoolValue(v))
        } else if let Some(v) = fields.take("intValue") {
            let Value::Integer(v) = v else {
                return None;
            };
            Some(PbValue::IntValue(v))
        } else if let Some(v) = fields.take("doubleValue") {
            let Value::Float(v) = v else {
                return None;
            };
            Some(PbValue::DoubleValue(v.into_inner()))
        } else if let Some(v) = fields.take("bytesValue") {
            let bytes = v.as_bytes()?;
            Some(PbValue::BytesValue(bytes.to_vec()))
        } else if let Some(v) = fields.take("arrayValue") {
            let Value::Object(object) = v else {
                return None;
            };
            let mut inner = ConsumeFields::wrap(object, fields.reporter);
            let values = into_entries(inner.take("values"))
                .into_iter()
                .filter_map(|item| {
                    let parsed = Self::from_value(item, dropped, inner.reporter);
                    count_malformed_attribute(parsed, dropped, inner.reporter)
                })
                .collect();
            inner.finish();
            Some(PbValue::ArrayValue(ArrayValue { values }))
        } else if let Some(v) = fields.take("kvlistValue") {
            let Value::Object(object) = v else {
                return None;
            };
            let mut inner = ConsumeFields::wrap(object, fields.reporter);
            let values = inner.take("values");
            let values = array_to_key_values(values, dropped, inner.reporter);
            inner.finish();
            Some(PbValue::KvlistValue(KeyValueList { values }))
        } else {
            None
        };
        fields.finish();
        Some(Self { value })
    }

    /// Parses a flattened attribute value. Scalar strings may be `Value::Bytes` or
    /// `Value::String`; valid UTF-8 bytes become a string. Values with no `AnyValue` form fail.
    fn from_flattened(
        value: Value,
        dropped: &mut u32,
        reporter: &mut impl TraceConversionReporter,
    ) -> Option<Self> {
        let value = match value {
            Value::Null => None,
            Value::Bytes(bytes) => Some(match String::from_utf8(bytes.to_vec()) {
                Ok(s) => PbValue::StringValue(s),
                Err(err) => PbValue::BytesValue(err.into_bytes()),
            }),
            Value::String(s) => Some(PbValue::StringValue(s.into())),
            Value::Boolean(v) => Some(PbValue::BoolValue(v)),
            Value::Integer(v) => Some(PbValue::IntValue(v)),
            Value::Float(v) => Some(PbValue::DoubleValue(v.into_inner())),
            Value::Array(items) => Some(PbValue::ArrayValue(ArrayValue {
                values: items
                    .into_iter()
                    .filter_map(|item| {
                        let parsed = Self::from_flattened(item, dropped, reporter);
                        count_malformed_attribute(parsed, dropped, reporter)
                    })
                    .collect(),
            })),
            Value::Object(object) => Some(PbValue::KvlistValue(KeyValueList {
                values: flattened_key_values(object, dropped, reporter),
            })),
            Value::Regex(_) | Value::Timestamp(_) => return None,
        };
        Some(Self { value })
    }
}

/// Parses a `KeyValue` list, adding malformed entries (at any nesting depth) to `dropped`, the
/// enclosing item's in-band count.
fn array_to_key_values(
    value: Option<Value>,
    dropped: &mut u32,
    reporter: &mut impl TraceConversionReporter,
) -> Vec<KeyValue> {
    let (key_values, malformed) = parse_entries(
        into_entries(value),
        TraceConversionIssue::MalformedAttribute,
        reporter,
        |value, reporter| KeyValue::from_value(value, dropped, reporter),
    );
    *dropped = dropped.saturating_add(malformed);
    key_values
}

/// Parses flattened attributes, which the source stores as an object. A present non-object
/// value counts as one malformed attribute.
fn object_to_key_values(
    value: Option<Value>,
    dropped: &mut u32,
    reporter: &mut impl TraceConversionReporter,
) -> Vec<KeyValue> {
    match value {
        None => Vec::new(),
        Some(Value::Object(object)) => flattened_key_values(object, dropped, reporter),
        Some(_) => {
            count_malformed_attribute(None::<()>, dropped, reporter);
            Vec::new()
        }
    }
}

fn flattened_key_values(
    object: ObjectMap,
    dropped: &mut u32,
    reporter: &mut impl TraceConversionReporter,
) -> Vec<KeyValue> {
    object
        .into_iter()
        .filter_map(|(key, value)| {
            let parsed = AnyValue::from_flattened(value, dropped, reporter);
            let value = count_malformed_attribute(parsed, dropped, reporter)?;
            Some(KeyValue {
                key: key.into(),
                value: Some(value),
            })
        })
        .collect()
}

/// True when `value` is byte-like (`Value::Bytes` or `Value::String`) and non-empty.
pub(super) fn is_nonempty_bytes(value: &Value) -> bool {
    value.as_bytes().is_some_and(|bytes| !bytes.is_empty())
}

fn into_utf8(value: Value) -> Option<String> {
    match value {
        Value::Bytes(bytes) => String::from_utf8(bytes.to_vec()).ok(),
        Value::String(s) => Some(s.into()),
        _ => None,
    }
}

fn decode_hex_bytes(value: &Value) -> Option<Vec<u8>> {
    value.as_bytes().and_then(|bytes| hex::decode(bytes).ok())
}

/// Reads a `resourceSpans` ID, which the protobuf decoder stores as raw big-endian bytes.
///
/// Returns empty when absent; the shared span conversion rejects and counts malformed input.
fn raw_id(value: Option<Value>) -> Vec<u8> {
    match value {
        None => Vec::new(),
        Some(value) => value
            .as_bytes()
            .map_or_else(|| INVALID_ID.to_vec(), |bytes| bytes.to_vec()),
    }
}

/// Reads a flattened-layout ID, which the source stores as hex.
///
/// Returns empty when absent; the shared span conversion rejects and counts malformed input.
fn hex_id(value: Option<Value>) -> Vec<u8> {
    match value {
        None => Vec::new(),
        Some(v) => decode_hex_bytes(&v).unwrap_or_else(|| INVALID_ID.to_vec()),
    }
}
