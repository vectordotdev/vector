use metrics::{register_counter, Counter};
use vector_common::internal_event::{Count, Registered};

use crate::{
    internal_events::{ComponentEventsDropped, INTENTIONAL},
    register,
};

vector_common::registered_event! (
    FilterEventsDropped => Handle {
        events_dropped: Registered<ComponentEventsDropped<'static, INTENTIONAL>>,
        events_discarded: Counter,
    }

    fn register(self) {
        Handle {
            events_dropped: register!(ComponentEventsDropped::<INTENTIONAL>::from(
                "Events matched filter condition."
            )),
            events_discarded: register_counter!("events_discarded_total"),
        }
    }

    fn emit(&self, data: Count) {
        self.events_dropped.emit(data);
        self.events_discarded.increment(data.0 as u64);
    }
);
