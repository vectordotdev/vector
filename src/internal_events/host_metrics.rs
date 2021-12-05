use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct HostMetricsEventReceived {
    pub count: usize,
}

impl InternalEvent for HostMetricsEventReceived {
    fn emit_logs(&self) {
        debug!(message = "Scraped host metrics.", count = ?self.count);
    }
}
