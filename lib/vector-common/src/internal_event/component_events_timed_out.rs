use metrics::{Counter, counter};

use super::{Count, InternalEvent, InternalEventHandle, RegisterInternalEvent};

#[derive(Debug)]
pub struct ComponentEventsTimedOut<'a> {
    pub count: usize,
    pub reason: &'a str,
}

impl InternalEvent for ComponentEventsTimedOut<'_> {
    fn emit(self) {
        let count = self.count;
        self.register().emit(Count(count));
    }

    fn name(&self) -> Option<&'static str> {
        Some("ComponentEventsTimedOut")
    }
}

impl<'a> From<&'a str> for ComponentEventsTimedOut<'a> {
    fn from(reason: &'a str) -> Self {
        Self { count: 0, reason }
    }
}

impl<'a> RegisterInternalEvent for ComponentEventsTimedOut<'a> {
    type Handle = TimedOutHandle<'a>;
    fn register(self) -> Self::Handle {
        Self::Handle {
            timed_out_events: counter!("component_timedout_events_total"),
            timed_out_requests: counter!("component_timedout_requests_total"),
            reason: self.reason,
        }
    }
}

#[derive(Clone)]
pub struct TimedOutHandle<'a> {
    timed_out_events: Counter,
    timed_out_requests: Counter,
    reason: &'a str,
}

impl InternalEventHandle for TimedOutHandle<'_> {
    type Data = Count;
    fn emit(&self, data: Self::Data) {
        warn!(
            message = "Events timed out",
            events = data.0,
            reason = self.reason,
        );
        self.timed_out_events.increment(data.0 as u64);
        self.timed_out_requests.increment(1);
    }
}
