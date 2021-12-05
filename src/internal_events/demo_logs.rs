use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DemoLogsEventProcessed;

impl InternalEvent for DemoLogsEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }
}
