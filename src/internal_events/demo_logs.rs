use vector_lib::{NamedInternalEvent, internal_event::InternalEvent};

#[derive(Debug, NamedInternalEvent)]
pub(crate) struct DemoLogsEventProcessed;

impl InternalEvent for DemoLogsEventProcessed {
    fn emit(self) {
        trace!(message = "Received one event.");
    }
}
