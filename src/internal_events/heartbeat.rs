use super::InternalEvent;
use metrics::gauge;
use std::time::Instant;

#[derive(Debug)]
pub struct Heartbeat {
    pub since: Instant,
}

impl InternalEvent for Heartbeat {
    fn emit_logs(&self) {
        trace!(target: "vector", message = "Beep.");
    }

    fn emit_metrics(&self) {
        gauge!("uptime_seconds", self.since.elapsed().as_secs() as i64);
    }
}
