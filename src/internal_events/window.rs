use vector_lib::internal_event::{ComponentEventsDropped, Count, Registered, INTENTIONAL};

vector_lib::registered_event!(
    WindowEventsDropped => {
        events_dropped: Registered<ComponentEventsDropped<'static, INTENTIONAL>>
            = register!(ComponentEventsDropped::<INTENTIONAL>::from(
                "The buffer was full"
            )),
    }

    fn emit(&self, data: Count) {
        self.events_dropped.emit(data);
    }
);
