use vector_lib::internal_event::{ComponentEventsDropped, INTENTIONAL, InternalEvent};

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
