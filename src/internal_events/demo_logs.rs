use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct DemoLogsEventProcessed;

impl InternalEvent for DemoLogsEventProcessed {
    fn emit(self) {
        trace!(message = "Received one event.");
    }
}
