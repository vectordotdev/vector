use std::{cell::RefCell, collections::HashSet};

use itertools::Itertools as _;

thread_local! {
    /// A buffer for recording internal events emitted by a single test.
    static EVENTS_RECORDED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Returns Ok(()) if the event name pattern is matched only once.
///
/// # Errors
///
/// Will return `Err` if `pattern` is not found in the event record, or is found multiple times.
pub fn contains_name_once(pattern: &str) -> Result<(), String> {
    EVENTS_RECORDED.with(|events| {
        let events = events.borrow();
        let matches: Vec<_> = events
            .iter()
            .filter(|event| event_name_matches(event, pattern))
            .collect();
        match matches.len() {
            0 => Err(format!("Missing event {pattern:?}")),
            1 => Ok(()),
            n => {
                let names = matches
                    .into_iter()
                    .map(|event| format!("{event:?}"))
                    .join(", ");
                Err(format!(
                    "Multiple ({n}) events matching {pattern:?}: ({names}). Hint! Don't use the `assert_x_` test \
                     helpers on round-trip tests (tests that run more than a single component)."
                ))
            }
        }
    })
}

pub fn clear_recorded_events() {
    EVENTS_RECORDED.with(|er| er.borrow_mut().clear());
}

#[allow(clippy::print_stdout)]
pub fn debug_print_events() {
    EVENTS_RECORDED.with(|events| {
        for event in &*events.borrow() {
            println!("{event}");
        }
    });
}

fn event_name_matches(event: &str, pattern: &str) -> bool {
    let segment = event.rsplit_once("::").map_or(event, |(_, suffix)| suffix);
    segment == pattern || (segment.ends_with(pattern) && !ignore_prefixed_match(segment, pattern))
}

fn ignore_prefixed_match(segment: &str, pattern: &str) -> bool {
    // Buffer telemetry emits its own `BufferEvents{{Received|Sent}}` events for destinations in the
    // topology. Component compliance only cares about the component-scoped
    // `Events{{Received|Sent}}` signals, so we explicitly filter out the buffer-prefixed
    // forms when matching these shared names. Other prefixes remain eligible.
    matches!(pattern, "EventsReceived" | "EventsSent") && segment.starts_with("Buffer")
}

/// Record an emitted internal event. This is somewhat dumb at this
/// point, just recording the pure string value of the `emit!` call
/// parameter. At some point, making all internal events implement
/// `Debug` or `Serialize` might allow for more sophistication here, but
/// this is good enough for these tests. This should only be used by the
/// test `emit!` macro. The `check-events` script will test that emitted
/// events contain the right fields, etc.
pub fn record_internal_event(event: &str) {
    // Remove leading '&'
    let event = event.strip_prefix('&').unwrap_or(event);
    // Remove trailing '{fields…}'
    let event = event.find('{').map_or(event, |par| &event[..par]);
    // Remove trailing '::from…'
    let event = event.find(':').map_or(event, |colon| &event[..colon]);

    EVENTS_RECORDED.with(|er| er.borrow_mut().insert(event.trim().into()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset_events() {
        clear_recorded_events();
    }

    fn insert_raw_event(event: &str) {
        super::EVENTS_RECORDED.with(|events| {
            events.borrow_mut().insert(event.into());
        });
    }

    #[test]
    fn contains_name_once_accepts_exact_match() {
        reset_events();
        record_internal_event("EventsReceived");
        assert!(contains_name_once("EventsReceived").is_ok());
    }

    #[test]
    fn contains_name_once_ignores_prefix_matches() {
        reset_events();
        record_internal_event("EventsReceived");
        record_internal_event("BufferEventsReceived");

        assert!(contains_name_once("EventsReceived").is_ok());
    }

    #[test]
    fn contains_name_once_matches_module_qualified_names() {
        reset_events();
        insert_raw_event("vector::internal_events::EventsSent");

        assert!(contains_name_once("EventsSent").is_ok());
    }

    #[test]
    fn contains_name_once_still_flags_multiple_exact_matches() {
        reset_events();
        record_internal_event("EventsSent");
        insert_raw_event("vector::internal_events::EventsSent");

        let err = contains_name_once("EventsSent").unwrap_err();
        assert!(
            err.contains("Multiple (2) events matching \"EventsSent\""),
            "{err}"
        );
    }

    #[test]
    fn contains_name_once_matches_prefixed_component_events() {
        reset_events();
        record_internal_event("SocketEventsReceived");

        assert!(contains_name_once("EventsReceived").is_ok());
    }

    #[test]
    fn contains_name_once_ignores_buffer_prefixed_events() {
        reset_events();
        record_internal_event("BufferEventsReceived");

        assert!(contains_name_once("EventsReceived").is_err());
    }
}
