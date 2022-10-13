use super::InternalEvent;
use metrics::counter;

#[allow(dead_code)]
pub const INTENTIONAL: bool = true;
pub const UNINTENTIONAL: bool = false;

#[derive(Debug)]
pub struct ComponentEventsDropped<const INTENTIONAL: bool> {
    pub count: u64,
    pub reason: &'static str,
}

impl<const INTENTIONAL: bool> InternalEvent for ComponentEventsDropped<INTENTIONAL> {
    fn emit(self) {
        let message = "Events dropped";
        if INTENTIONAL {
            debug!(
                message,
                intentional = INTENTIONAL,
                reason = self.reason,
                count = self.count,
                internal_log_rate_secs = true,
            );
        } else {
            error!(
                message,
                intentional = INTENTIONAL,
                reason = self.reason,
                count = self.count,
                internal_log_rate_secs = true,
            );
        }
        counter!(
            "component_discarded_events_total",
            self.count,
            "intentional" => if INTENTIONAL { "true" } else { "false" },
        );
    }
}
