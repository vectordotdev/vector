use metrics::{register_counter, Counter};
use tracing::trace;

use super::{CountByteSize, InternalEventHandle, Output, RegisterInternalEvent, SharedString};

pub const DEFAULT_OUTPUT: &str = "_default";

#[derive(Debug)]
pub struct EventsSent {
    pub output: Option<SharedString>,
}

impl From<Output> for EventsSent {
    fn from(output: Output) -> Self {
        Self { output: output.0 }
    }
}

impl RegisterInternalEvent for EventsSent {
    type Handle = EventsSentHandle;

    fn register(self) -> Self::Handle {
        if let Some(output) = self.output {
            EventsSentHandle {
                events: register_counter!("component_sent_events_total", "output" => output.clone()),
                events_out: register_counter!("events_out_total", "output" => output.clone()),
                event_bytes: register_counter!("component_sent_event_bytes_total", "output" => output.clone()),
                output: Some(output),
            }
        } else {
            EventsSentHandle {
                events: register_counter!("component_sent_events_total"),
                events_out: register_counter!("events_out_total"),
                event_bytes: register_counter!("component_sent_event_bytes_total"),
                output: None,
            }
        }
    }

    fn name(&self) -> Option<&'static str> {
        Some("EventsSent")
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Clone)]
pub struct EventsSentHandle {
    events: Counter,
    events_out: Counter,
    event_bytes: Counter,
    output: Option<SharedString>,
}

impl InternalEventHandle for EventsSentHandle {
    type Data = CountByteSize;

    fn emit(&self, data: Self::Data) {
        let CountByteSize(count, byte_size) = data;

        match &self.output {
            Some(output) => {
                trace!(message = "Events sent.", count = %count, byte_size = %byte_size, output = %output);
            }
            None => {
                trace!(message = "Events sent.", count = %count, byte_size = %byte_size);
            }
        }

        self.events.increment(count as u64);
        self.events_out.increment(count as u64);
        self.event_bytes.increment(byte_size as u64);
    }
}
