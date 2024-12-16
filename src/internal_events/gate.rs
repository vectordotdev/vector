use vector_lib::internal_event::{ComponentEventsDropped, Count, INTENTIONAL, Registered};

vector_lib::registered_event!(
    GateEventsDropped => {
        events_dropped: Registered<ComponentEventsDropped<'static, INTENTIONAL>>
            = register!(ComponentEventsDropped::<INTENTIONAL>::from(
                "The gate was closed."
            )),
    }

    fn emit(&self, data: Count) {
        self.events_dropped.emit(data);
    }
);
