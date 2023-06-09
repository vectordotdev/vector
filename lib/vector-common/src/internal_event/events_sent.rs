use metrics::{register_counter, Counter};
use tracing::trace;

use super::{CountByteSize, OptionalTag, Output, SharedString};

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

/// Makes a list of the tags to use with the events sent event.
fn make_tags(
    output: &Option<SharedString>,
    source: &OptionalTag,
    service: &OptionalTag,
) -> Vec<(&'static str, String)> {
    let mut tags = Vec::new();
    if let Some(output) = &output {
        tags.push(("output", output.to_string()));
    }

    if let OptionalTag::Specified(tag) = source {
        tags.push(("source", tag.clone().unwrap_or("-".to_string())));
    }

    if let OptionalTag::Specified(tag) = service {
        tags.push(("service", tag.clone().unwrap_or("-".to_string())));
    }

    tags
}

crate::registered_event!(
    TaggedEventsSent {
        output: Option<SharedString>,
        source: OptionalTag,
        service: OptionalTag,
    } => {
        events: Counter = {
            register_counter!("component_sent_events_total", &make_tags(&self.output, &self.source, &self.service))
        },
        events_out: Counter = if let Some(output) = &self.output {
            register_counter!("events_out_total", "output" => output.clone())
        } else {
            register_counter!("events_out_total")
        },
        event_bytes: Counter = {
            register_counter!("component_sent_event_bytes_total", &make_tags(&self.output, &self.source, &self.service))
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

    fn register(tags: &(OptionalTag, OptionalTag)) {
        super::register(TaggedEventsSent::new(
            tags.0.clone(),
            tags.1.clone(),
            Output(None),
        ))
    }
);

/// TODO: This can probably become a part of the previous macro.
// impl RegisterEvent<(OptionalTag, OptionalTag)> for TaggedEventsSent {
//     fn register(
//         tags: &(OptionalTag, OptionalTag),
//     ) -> <TaggedEventsSent as super::RegisterInternalEvent>::Handle {
//         super::register(TaggedEventsSent::new(
//             tags.0.clone(),
//             tags.1.clone(),
//             Output(None),
//         ))
//     }
// }

impl TaggedEventsSent {
    #[must_use]
    pub fn new(source: OptionalTag, service: OptionalTag, output: Output) -> Self {
        Self {
            output: output.0,
            source,
            service,
        }
    }
}
