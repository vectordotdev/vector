use metrics::Counter;

use crate::counter;

use super::{Count, MetricName};

crate::registered_event! {
    ComponentEventsTimedOut {
        reason: &'static str,
    } => {
        timed_out_events: Counter = counter!(MetricName::ComponentTimedOutEventsTotal),
        timed_out_requests: Counter = counter!(MetricName::ComponentTimedOutRequestsTotal),
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
