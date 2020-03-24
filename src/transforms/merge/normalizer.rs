use super::{MaybePartialLogEvent, NormalizeLogEvent};
use crate::event::{LogEvent, Value};
use string_cache::DefaultAtom as Atom;

/// A [`NormalizeLogEvent`] implementation that performs pariality detection
/// based on the presence of the `partial_event_marker_field`.
///
/// If the event has `partial_event_marker_field` among it's field, then,
/// regardless of the value
#[derive(Debug, Clone, PartialEq)]
pub struct PartialEventMarkerFieldNormalizer {
    pub partial_event_marker_field: Atom,
}

impl NormalizeLogEvent for PartialEventMarkerFieldNormalizer {
    fn normalize(&self, mut event: LogEvent) -> MaybePartialLogEvent {
        // If the event has a field with `partial_event_marker_field` key -
        // it is a partial event. It is expected that we remove the partial
        // event marker - so do both the removal and check efficiently in a
        // single operation.
        if event.remove(&self.partial_event_marker_field).is_some() {
            MaybePartialLogEvent::Partial(event)
        } else {
            MaybePartialLogEvent::NonPartial(event)
        }
    }
}

/// A [`NormalizeLogEvent`] implementation that performs pariality detection
/// based on the presence of the trailing newline at the `probe_field` of the
/// event.
///
/// If the event has `probe_field` field, and it's a string value that DOES NOT
/// contain a newline (`\n`) at the end, we consider that event partial.
/// To normalize the event, we remove the trailing newline.
/// If the event
/// - has no `probe_field` field, or
/// - has it, but the values is not a string, or
/// - it is a string that DOES contain a trailing newline
/// we consider the event non-partial.
#[derive(Debug, Clone, PartialEq)]
pub struct TrailingNewlineNormalizer {
    pub probe_field: Atom,
}

impl NormalizeLogEvent for TrailingNewlineNormalizer {
    fn normalize(&self, mut event: LogEvent) -> MaybePartialLogEvent {
        // See `TrailingNewlineNormalizer` documentation for logic description.

        if let Some(Value::Bytes(s)) = event.get_mut(&self.probe_field) {
            if s.ends_with(&[b'\n']) {
                s.truncate(s.len() - 1);
            } else {
                return MaybePartialLogEvent::Partial(event);
            }
        }

        MaybePartialLogEvent::NonPartial(event)
    }
}
