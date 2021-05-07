use super::InternalEvent;

#[derive(Debug)]
pub(crate) struct HostMetricsEventReceived {
    pub count: usize,
}

impl InternalEvent for HostMetricsEventReceived {
    fn emit_logs(&self) {
        debug!(message = "Scraped host metrics.", count = ?self.count);
    }
}
