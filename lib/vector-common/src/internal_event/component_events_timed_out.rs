use metrics::{Counter, counter};

use super::Count;

crate::registered_event! {
    ComponentEventsTimedOut {
        reason: &'static str,
    } => {
        timed_out_events: Counter = counter!("component_timed_out_events_total"),
        timed_out_requests: Counter = counter!("component_timed_out_requests_total"),
        reason: &'static str = self.reason,
    }

    fn emit(&self, data: Count) {
        warn!(
            message = "Events timed out",
            events = data.0,
            reason = self.reason,
        );
        self.timed_out_events.increment(data.0 as u64);
        self.timed_out_requests.increment(1);
    }
}
