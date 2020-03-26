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

#[cfg(test)]
mod tests {
    use super::{
        MaybePartialLogEvent, NormalizeLogEvent, PartialEventMarkerFieldNormalizer,
        TrailingNewlineNormalizer,
    };
    use crate::event::Event;

    #[test]
    fn partial_event_marker_field_normalizer_non_partial() {
        let partial_event_marker_field = "_partial";

        let normalizer = PartialEventMarkerFieldNormalizer {
            partial_event_marker_field: partial_event_marker_field.into(),
        };

        let sample_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert("a", "qwe");
            event.insert("b", 1);
            // no partial event marker field - non-partial event
            event
        };

        let expected_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert("a", "qwe");
            event.insert("b", 1);
            // no partial event marker field - non-partial event
            event
        };

        assert_eq!(
            normalizer.normalize(sample_event),
            MaybePartialLogEvent::NonPartial(expected_event)
        );
    }

    #[test]
    fn partial_event_marker_field_normalizer_partial() {
        let partial_event_marker_field = "_partial";

        let normalizer = PartialEventMarkerFieldNormalizer {
            partial_event_marker_field: partial_event_marker_field.into(),
        };

        let sample_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert("a", "qwe");
            event.insert("b", 1);
            event.insert(partial_event_marker_field, true); // partial
            event
        };

        let expected_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert("a", "qwe");
            event.insert("b", 1);
            // doesn't have `partial_event_marker_field` anymore - normailized
            event
        };

        assert_eq!(
            normalizer.normalize(sample_event),
            MaybePartialLogEvent::Partial(expected_event)
        );
    }

    #[test]
    fn trailing_newline_normalizer_non_partial() {
        let message_field = "message";

        let normalizer = TrailingNewlineNormalizer {
            probe_field: message_field.into(),
        };

        let sample_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert(message_field, "hello world!\n"); // has trailing newline - non-partial
            event
        };

        let expected_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert(message_field, "hello world!"); // normalized - doesn't have a trailing newline
            event
        };

        assert_eq!(
            normalizer.normalize(sample_event),
            MaybePartialLogEvent::NonPartial(expected_event)
        );
    }

    #[test]
    fn trailing_newline_normalizer_partial() {
        let message_field = "message";

        let normalizer = TrailingNewlineNormalizer {
            probe_field: message_field.into(),
        };

        let sample_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert(message_field, "hello "); // ..." world!\n" - partial message
            event
        };

        let expected_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert(message_field, "hello "); // partial!
            event
        };

        assert_eq!(
            normalizer.normalize(sample_event),
            MaybePartialLogEvent::Partial(expected_event)
        );
    }

    #[test]
    fn trailing_newline_normalizer_special_case_bare_newline() {
        let message_field = "message";

        let normalizer = TrailingNewlineNormalizer {
            probe_field: message_field.into(),
        };

        let sample_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert(message_field, "\n");
            event
        };

        let expected_event = {
            let mut event = Event::new_empty_log().into_log();
            event.insert(message_field, "");
            event
        };

        assert_eq!(
            normalizer.normalize(sample_event),
            MaybePartialLogEvent::NonPartial(expected_event)
        );
    }
}
