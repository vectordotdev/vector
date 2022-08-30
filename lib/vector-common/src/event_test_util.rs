use std::{cell::RefCell, collections::HashSet};

thread_local! {
    /// A buffer for recording internal events emitted by a single test.
    static EVENTS_RECORDED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Ok(()) if true
///
/// # Errors
///
/// Will return `Err` if `name` is not found in the event record, or is found more than once.
pub fn contains_name_once(name: &str) -> Result<(), &'static str> {
    EVENTS_RECORDED.with(|events| {
        let mut found = false;
        for event in events.borrow().iter() {
            if event.ends_with(name) {
                if found {
                    return Err("Multiple events");
                }
                found = true;
            }
        }
        if found {
            Ok(())
        } else {
            Err("Missing event")
        }
    })
}

pub fn clear_recorded_events() {
    EVENTS_RECORDED.with(|er| er.borrow_mut().clear());
}

#[allow(clippy::print_stdout)]
pub fn debug_print_events() {
    EVENTS_RECORDED.with(|events| {
        for event in events.borrow().iter() {
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
    let event = event.strip_prefix('&').unwrap_or(event);
    // Remove trailing '{fields…}'
    let event = event.find('{').map_or(event, |par| &event[..par]);
    // Remove trailing '::from…'
    let event = event.find(':').map_or(event, |colon| &event[..colon]);

    EVENTS_RECORDED.with(|er| er.borrow_mut().insert(event.trim().into()));
}
