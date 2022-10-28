use metrics::{register_counter, Counter};
use vector_common::internal_event::{
    Count, InternalEventHandle, RegisterInternalEvent, Registered,
};

use crate::{
    internal_events::{ComponentEventsDropped, INTENTIONAL},
    register,
};

#[derive(Debug)]
pub struct FilterEventsDropped;

impl RegisterInternalEvent for FilterEventsDropped {
    type Handle = FilterEventsDroppedHandle;
    fn register(self) -> Self::Handle {
        Self::Handle {
            events_dropped: register!(ComponentEventsDropped::<INTENTIONAL>::from(
                "Events matched filter condition."
            )),
            events_discarded: register_counter!("events_discarded_total"),
        }
    }
}

#[derive(Clone)]
pub struct FilterEventsDroppedHandle {
    events_dropped: Registered<ComponentEventsDropped<'static, INTENTIONAL>>,
    events_discarded: Counter,
}

impl InternalEventHandle for FilterEventsDroppedHandle {
    type Data = Count;
    fn emit(&self, data: Count) {
        self.events_dropped.emit(data);
        self.events_discarded.increment(data.0 as u64);
    }
}
