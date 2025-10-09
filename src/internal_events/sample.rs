use vector_config::internal_event;
use vector_lib::internal_event::{ComponentEventsDropped, INTENTIONAL, InternalEvent};

#[internal_event]
#[derive(Debug)]
pub struct SampleEventDiscarded;

impl InternalEvent for SampleEventDiscarded {
    fn emit(self) {
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "Sample discarded."
        })
    }
}
