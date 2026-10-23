//! Stage-1 hinted OTLP legacy converter and historical layout detector.
//!
//! The converter does not build typed events itself. It parses each legacy layout back into
//! the protobuf types it was decoded from (`ResourceSpans` and its children) and hands them to
//! the shared decoding in `convert`. Every OTLP mapping rule (trace-ID partitioning,
//! semantic-convention promotion, Datadog bridge keys, duplicate keys, and status and timing
//! normalization) therefore applies identically to legacy records and to wire input. The
//! parsers here only recover protobuf values; fields a layout does not carry take their
//! protobuf defaults.
//!
//! Conversion consumes the legacy event and moves its values into those protobufs. A failed
//! conversion returns that event with the [`ConversionError`], so the caller can reject it.
//!
//! Every value the converter cannot carry into the typed model is reported: unmapped keys as
//! [`TraceConversionIssue::UnmappedField`], wrong-typed known fields as
//! [`TraceConversionIssue::MalformedField`] (with the field's default used instead), and
//! malformed list entries through their item-level issue.

mod parse;

#[cfg(test)]
mod tests;

use std::collections::btree_map::Entry;

use vector_core::event::{
    EventMetadata, TraceEvent as LegacyTraceEvent, TraceLayout,
    typed_trace::{TraceConversionIssue, TraceConversionReporter, TraceEvent},
};
use vrl::value::{ObjectMap, Value};

use crate::{
    proto::{RESOURCE_SPANS_JSON_FIELD, trace::v1::ResourceSpans},
    spans::{SPAN_ID_KEY, TRACE_ID_KEY},
};

use parse::is_nonempty_bytes;
use parse::{ConsumeFields, parse_entries};

/// Failure converting a legacy OTLP-shaped trace event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversionError {
    /// The event is not a recognized OTLP historical layout.
    NotOtlpLayout,
    /// The event carries a layout hint this Vector does not recognize.
    UnrecognizedLayout {
        /// Unrecognized protobuf layout number.
        layout: i32,
    },
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOtlpLayout => {
                f.write_str("legacy trace event is not a recognized OTLP layout")
            }
            Self::UnrecognizedLayout { layout } => {
                write!(
                    f,
                    "legacy trace event has unrecognized layout hint {layout}"
                )
            }
        }
    }
}

impl std::error::Error for ConversionError {}

/// A failed legacy conversion and the event the caller still has to reject.
///
/// The event is first. [`Self::error`] is the [`ConversionError`] reason.
#[derive(Clone, Debug, PartialEq)]
pub struct FailedConversion(LegacyTraceEvent, ConversionError);

impl FailedConversion {
    /// Why conversion failed.
    #[must_use]
    pub const fn error(&self) -> ConversionError {
        self.1
    }

    /// The event that failed to convert.
    #[must_use]
    pub fn into_event(self) -> LegacyTraceEvent {
        self.0
    }
}

impl std::fmt::Display for FailedConversion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.1, f)
    }
}

impl std::error::Error for FailedConversion {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.1)
    }
}

/// Outcome of [`hinted_legacy_to_typed`].
#[derive(Clone, Debug, PartialEq)]
pub enum HintedLegacy {
    /// The hint is absent or names another producer. The event is unchanged.
    Skipped(LegacyTraceEvent),
    /// The hint names an OTLP layout, or is unrecognized.
    Done(Result<Vec<TraceEvent>, FailedConversion>),
}

/// Returns `true` when `legacy` matches a historical OTLP layout (flattened per-span or
/// preserved `resourceSpans`) and does not look like Datadog source output.
#[must_use]
pub fn is_historical_otlp_layout(legacy: &LegacyTraceEvent) -> bool {
    legacy
        .value()
        .as_object()
        .is_some_and(|map| looks_like_otlp_envelope(map) || looks_like_flattened_otlp(map))
}

/// Converts an OTLP legacy event into typed events.
///
/// A layout hint selects the layout directly. Shape detection runs only for a hintless
/// record. Zero typed outputs is success (empty `ScopeSpans`, or every span rejected); drops
/// and normalizations go to `reporter`.
///
/// # Errors
///
/// Returns [`ConversionError::UnrecognizedLayout`] for an unrecognized hint, and
/// [`ConversionError::NotOtlpLayout`] when the hint names another producer or a hintless
/// record is neither a flattened per-span OTLP event nor a preserved `resourceSpans` batch.
/// The [`FailedConversion`] also returns the original event.
pub fn legacy_to_typed(
    legacy: LegacyTraceEvent,
    reporter: &mut impl TraceConversionReporter,
) -> Result<Vec<TraceEvent>, FailedConversion> {
    match hinted_legacy_to_typed(legacy, reporter) {
        HintedLegacy::Done(result) => result,
        HintedLegacy::Skipped(legacy) => match legacy.metadata().trace_layout() {
            Some(_) => Err(FailedConversion(legacy, ConversionError::NotOtlpLayout)),
            None => convert_by_shape(legacy, reporter),
        },
    }
}

/// Converts a hintless record using the layout its shape matches.
fn convert_by_shape(
    legacy: LegacyTraceEvent,
    reporter: &mut impl TraceConversionReporter,
) -> Result<Vec<TraceEvent>, FailedConversion> {
    let Some(map) = legacy.value().as_object() else {
        return Err(FailedConversion(legacy, ConversionError::NotOtlpLayout));
    };
    if looks_like_otlp_envelope(map) {
        convert_envelope(legacy, reporter)
    } else if looks_like_flattened_otlp(map) {
        legacy_map(legacy).map(|(map, metadata)| convert_flattened(map, &metadata, reporter))
    } else {
        Err(FailedConversion(legacy, ConversionError::NotOtlpLayout))
    }
}

/// Converts when the Stage 1 layout hint identifies an OTLP producer, using the layout the
/// hint names without shape detection.
///
/// Returns [`HintedLegacy::Skipped`] when the hint is absent or names another producer. An
/// unrecognized hint returns [`ConversionError::UnrecognizedLayout`] with the original event,
/// so callers never fall through to shape detection for it.
#[must_use]
pub fn hinted_legacy_to_typed(
    legacy: LegacyTraceEvent,
    reporter: &mut impl TraceConversionReporter,
) -> HintedLegacy {
    match legacy.metadata().trace_layout() {
        Some(TraceLayout::OtelFlattened) => HintedLegacy::Done(
            legacy_map(legacy).map(|(map, metadata)| convert_flattened(map, &metadata, reporter)),
        ),
        Some(TraceLayout::OtlpResourceSpans) => {
            HintedLegacy::Done(convert_envelope(legacy, reporter))
        }
        Some(TraceLayout::Unrecognized(layout)) => HintedLegacy::Done(Err(FailedConversion(
            legacy,
            ConversionError::UnrecognizedLayout { layout },
        ))),
        Some(TraceLayout::Datadog) | None => HintedLegacy::Skipped(legacy),
    }
}

/// Splits `legacy` into its field map and metadata.
///
/// Returns [`ConversionError::NotOtlpLayout`] and the original event when its root is not an
/// object.
fn legacy_map(legacy: LegacyTraceEvent) -> Result<(ObjectMap, EventMetadata), FailedConversion> {
    if legacy.value().as_object().is_none() {
        Err(FailedConversion(legacy, ConversionError::NotOtlpLayout))
    } else {
        Ok(legacy.into_parts())
    }
}

/// Removes `resourceSpans` when it is an array. Any other value stays in the map.
fn take_resource_spans(map: &mut ObjectMap) -> Option<Vec<Value>> {
    match map.entry(RESOURCE_SPANS_JSON_FIELD.into()) {
        Entry::Occupied(entry) if matches!(entry.get(), Value::Array(_)) => match entry.remove() {
            Value::Array(array) => Some(array),
            _ => unreachable!("the occupied value is an array"),
        },
        Entry::Occupied(_) | Entry::Vacant(_) => None,
    }
}

fn looks_like_otlp_envelope(map: &ObjectMap) -> bool {
    matches!(
        map.get(RESOURCE_SPANS_JSON_FIELD),
        Some(Value::Array(array)) if array.iter().all(|item| matches!(
            item,
            Value::Object(rs) if matches!(rs.get("scopeSpans"), None | Some(Value::Array(_)))
        ))
    )
}

fn looks_like_flattened_otlp(map: &ObjectMap) -> bool {
    !map.contains_key(RESOURCE_SPANS_JSON_FIELD)
        && !matches!(map.get("spans"), Some(Value::Array(_)))
        && has_id_field(map, TRACE_ID_KEY)
        && has_id_field(map, SPAN_ID_KEY)
        && map.contains_key("name")
        && map.contains_key("start_time_unix_nano")
}

fn has_id_field(map: &ObjectMap, key: &str) -> bool {
    map.get(key).is_some_and(is_nonempty_bytes)
}

fn convert_envelope(
    legacy: LegacyTraceEvent,
    reporter: &mut impl TraceConversionReporter,
) -> Result<Vec<TraceEvent>, FailedConversion> {
    let (mut map, metadata) = legacy_map(legacy)?;
    let Some(array) = take_resource_spans(&mut map) else {
        return Err(FailedConversion(
            LegacyTraceEvent::from_parts(map, metadata),
            ConversionError::NotOtlpLayout,
        ));
    };
    ConsumeFields::wrap(map, reporter).finish();
    let (resource_spans, _) = parse_entries(
        array,
        TraceConversionIssue::MalformedGrouping,
        reporter,
        ResourceSpans::from_value,
    );
    Ok(resource_spans
        .into_iter()
        .flat_map(|rs| rs.into_typed_events(&metadata, reporter))
        .collect())
}

fn convert_flattened(
    map: ObjectMap,
    metadata: &EventMetadata,
    reporter: &mut impl TraceConversionReporter,
) -> Vec<TraceEvent> {
    ResourceSpans::from_flattened_map(map, reporter).into_typed_events(metadata, reporter)
}
