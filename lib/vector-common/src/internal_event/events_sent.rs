use metrics::{register_counter, Counter};
use tracing::trace;

use super::{CountByteSize, Output, RegisterEvent, SharedString};

pub const DEFAULT_OUTPUT: &str = "_default";

crate::registered_event!(
    EventsSent {
        output: Option<SharedString>,
    } => {
        events: Counter = if let Some(output) = &self.output {
            register_counter!("component_sent_events_total", "output" => output.clone())
        } else {
            register_counter!("component_sent_events_total")
        },
        event_bytes: Counter = if let Some(output) = &self.output {
            register_counter!("component_sent_event_bytes_total", "output" => output.clone())
        } else {
            register_counter!("component_sent_event_bytes_total")
        },
        output: Option<SharedString> = self.output,
    }

    fn emit(&self, data: CountByteSize) {
        let CountByteSize(count, byte_size) = data;

        match &self.output {
            Some(output) => {
                trace!(message = "Events sent.", count = %count, byte_size = %byte_size.get(), output = %output);
            }
            None => {
                trace!(message = "Events sent.", count = %count, byte_size = %byte_size.get());
            }
        }

        self.events.increment(count as u64);
        self.event_bytes.increment(byte_size.get() as u64);
    }
);

impl From<Output> for EventsSent {
    fn from(output: Output) -> Self {
        Self { output: output.0 }
    }
}

crate::registered_event!(
    TaggedEventsSent {
        output: Option<SharedString>,
        source: Option<String>,
        service: Option<String>,
    } => {
        events: Counter = if let Some(output) = &self.output {
            register_counter!("component_sent_events_total", "output" => output.clone(),
                "source" => self.source.clone().unwrap_or("-".to_string()),
                "service" => self.service.clone().unwrap_or("-".to_string()))
        } else {
            register_counter!("component_sent_events_total",
                "source" => self.source.clone().unwrap_or("-".to_string()),
                "service" => self.service.clone().unwrap_or("-".to_string()))
        },
        events_out: Counter = if let Some(output) = &self.output {
            register_counter!("events_out_total", "output" => output.clone())
        } else {
            register_counter!("events_out_total")
        },
        event_bytes: Counter = if let Some(output) = &self.output {
            register_counter!("component_sent_event_bytes_total",
                "output" => output.clone(),
                "source" => self.source.clone().unwrap_or("-".to_string()),
                "service" => self.service.clone().unwrap_or("-".to_string()))
        } else {
            register_counter!("component_sent_event_bytes_total",
                "source" => self.source.clone().unwrap_or("-".to_string()),
                "service" => self.service.clone().unwrap_or("-".to_string()))
        },
        output: Option<SharedString> = self.output,
    }

    fn emit(&self, data: CountByteSize) {
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
        self.event_bytes.increment(byte_size.get() as u64);
    }
);

/// TODO: This can probably become a part of the previous macro.
impl RegisterEvent<(Option<String>, Option<String>)> for TaggedEventsSent {
    fn register(
        tags: &(Option<String>, Option<String>),
    ) -> <TaggedEventsSent as super::RegisterInternalEvent>::Handle {
        super::register(TaggedEventsSent::new(
            tags.0.clone(),
            tags.1.clone(),
            Output(None),
        ))
    }
}

impl TaggedEventsSent {
    #[must_use]
    pub fn new(source: Option<String>, service: Option<String>, output: Output) -> Self {
        Self {
            output: output.0,
            source,
            service,
        }
    }
}
