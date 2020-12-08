use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct HostMetricsEventReceived {
    pub count: usize,
}

impl InternalEvent for HostMetricsEventReceived {
    fn emit_logs(&self) {
        debug!(message = "Scraped host metrics.", count = ?self.count);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", self.count as u64);
    }
}
