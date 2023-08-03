use vector_common::internal_event::{ComponentEventsDropped, Count, Registered, INTENTIONAL};

use crate::register;

vector_common::registered_event! (
    FilterEventsDropped => {
        events_dropped: Registered<ComponentEventsDropped<'static, INTENTIONAL>>
            = register!(ComponentEventsDropped::<INTENTIONAL>::from(
                "Events matched filter condition."
            )),
    }

    fn emit(&self, data: Count) {
        self.events_dropped.emit(data);
    }
);
