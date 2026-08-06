use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent},
    json_size::JsonSize,
};

#[derive(Debug, NamedInternalEvent)]
pub struct InternalLogsBytesReceived {
    pub byte_size: usize,
}

impl InternalEvent for InternalLogsBytesReceived {
    fn emit(self) {
        // MUST NOT emit logs here to avoid an infinite log loop
        counter!(
            CounterName::ComponentReceivedBytesTotal,
            "protocol" => "internal",
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct InternalLogsEventsReceived {
    pub byte_size: JsonSize,
    pub count: usize,
}

impl InternalEvent for InternalLogsEventsReceived {
    fn emit(self) {
        // MUST NOT emit logs here to avoid an infinite log loop
        counter!(CounterName::ComponentReceivedEventsTotal).increment(self.count as u64);
        counter!(CounterName::ComponentReceivedEventBytesTotal)
            .increment(self.byte_size.get() as u64);
    }
}
