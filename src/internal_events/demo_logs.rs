use vector_lib::{NamedInternalEvent, internal_event::InternalEvent};

#[derive(Debug, NamedInternalEvent)]
pub struct DemoLogsEventProcessed;

impl InternalEvent for DemoLogsEventProcessed {
    fn emit(self) {
        trace!(message = "Received one event.");
    }
}
