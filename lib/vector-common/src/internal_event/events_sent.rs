use std::sync::Arc;

use metrics::{register_counter, Counter};
use tracing::trace;

use crate::config::ComponentKey;

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
    source: &OptionalTag<Arc<ComponentKey>>,
    service: &OptionalTag<String>,
) -> Vec<(&'static str, String)> {
    let mut tags = Vec::new();
    if let OptionalTag::Specified(tag) = source {
        tags.push((
            "source",
            tag.as_ref()
                .map_or_else(|| "-".to_string(), |tag| tag.id().to_string()),
        ));
    }

    if let OptionalTag::Specified(tag) = service {
        tags.push(("service", tag.clone().unwrap_or("-".to_string())));
    }

    tags
}

crate::registered_event!(
    TaggedEventsSent {
        source: OptionalTag<Arc<ComponentKey>>,
        service: OptionalTag<String>,
    } => {
        events: Counter = {
            register_counter!("component_sent_events_total", &make_tags(&self.source, &self.service))
        },
        event_bytes: Counter = {
            register_counter!("component_sent_event_bytes_total", &make_tags(&self.source, &self.service))
        },
    }

    fn emit(&self, data: CountByteSize) {
        let CountByteSize(count, byte_size) = data;
        trace!(message = "Events sent.", %count, %byte_size);

        self.events.increment(count as u64);
        self.event_bytes.increment(byte_size.get() as u64);
    }

    fn register(_fixed: (), tags: TaggedEventsSent) {
        super::register(tags)
    }
);

impl TaggedEventsSent {
    #[must_use]
    pub fn new_empty() -> Self {
        Self {
            source: OptionalTag::Specified(None),
            service: OptionalTag::Specified(None),
        }
    }

    #[must_use]
    pub fn new_unspecified() -> Self {
        Self {
            source: OptionalTag::Ignored,
            service: OptionalTag::Ignored,
        }
    }
}
