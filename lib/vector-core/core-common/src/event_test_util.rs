use std::{cell::RefCell, collections::HashSet};

thread_local!(
    /// A buffer for recording internal events emitted by a single test.
    static EVENTS_RECORDED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
);

#[must_use]
pub fn contains_name(name: &str) -> bool {
    EVENTS_RECORDED.with(|events| events.borrow().iter().any(|event| event.ends_with(name)))
}

pub fn clear_recorded_events() {
    EVENTS_RECORDED.with(|er| er.borrow_mut().clear());
}

pub fn debug_print_events() {
    EVENTS_RECORDED.with(|events| {
        for event in events.borrow().iter() {
            #![allow(clippy::print_stdout)] // that is the purpose of this function
            println!("{}", event);
        }
    });
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
    // Remove trailing '{fields…}'
    let event = event.strip_prefix('&').unwrap_or(event);
    let event = event.find('{').map_or(event, |par| &event[..par]);
    EVENTS_RECORDED.with(|er| er.borrow_mut().insert(event.into()));
}
