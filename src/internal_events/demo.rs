use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DemoEventProcessed;

impl InternalEvent for DemoEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }
}
