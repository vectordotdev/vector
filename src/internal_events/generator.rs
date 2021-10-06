use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct GeneratorEventProcessed;

impl InternalEvent for GeneratorEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Received one event.");
    }
}
