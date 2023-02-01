use metrics::{register_counter, Counter};
use vector_common::internal_event::{ComponentEventsDropped, Count, Registered, INTENTIONAL};

use crate::register;

vector_common::registered_event! (
    FilterEventsDropped => {
        events_dropped: Registered<ComponentEventsDropped<'static, INTENTIONAL>>
            = register!(ComponentEventsDropped::<INTENTIONAL>::from(
                "Events matched filter condition."
            )),
        events_discarded: Counter = register_counter!("events_discarded_total"),
    }

    fn emit(&self, data: Count) {
        self.events_dropped.emit(data);
        self.events_discarded.increment(data.0 as u64);
    }
);
