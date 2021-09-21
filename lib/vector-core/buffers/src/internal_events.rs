use internal_event::InternalEvent;

pub struct EventsReceived {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for EventsReceived {
    fn emit_logs(&self) {}

    fn emit_metrics(&self) {}
}

pub struct EventsSent {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for EventsSent {
    fn emit_logs(&self) {}

    fn emit_metrics(&self) {}
}

pub struct EventsDropped {
    pub count: usize,
}

impl InternalEvent for EventsDropped {
    fn emit_logs(&self) {}

    fn emit_metrics(&self) {}
}
