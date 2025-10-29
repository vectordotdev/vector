use vector_config::internal_event;
use vector_lib::internal_event::InternalEvent;

#[internal_event]
#[derive(Debug)]
pub struct DemoLogsEventProcessed;

impl InternalEvent for DemoLogsEventProcessed {
    fn emit(self) {
        trace!(message = "Received one event.");
    }
}
