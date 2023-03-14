use std::collections::HashMap;

use metrics::{register_counter, Counter};
use tracing::trace;

use crate::config::ComponentKey;

use super::{register, CountByteSize, Output, SharedString, Source};

pub const DEFAULT_OUTPUT: &str = "_default";

crate::registered_event!(
    EventsSent {
        output: Option<SharedString>,
        source: Option<SharedString>,
    } => {
        events: Counter = match (&self.output, &self.source) {
            (None, None) => register_counter!("component_sent_events_total"),
            (Some(output), None) => register_counter!("component_sent_events_total", "output" => output.clone()),
            (None, Some(source)) => register_counter!("component_sent_events_total", "source" => source.clone()),
            (Some(output), Some(source)) => register_counter!("component_sent_events_total", "output" => output.clone(), "source" => source.clone()),
        },

        events_out: Counter = match (&self.output, &self.source) {
            (None, None) => register_counter!("events_out_total"),
            (Some(output), None) => register_counter!("events_out_total", "output" => output.clone()),
            (None, Some(source)) => register_counter!("events_out_total", "source" => source.clone()),
            (Some(output), Some(source)) => register_counter!("events_out_total", "output" => output.clone(), "source" => source.clone()),
        },

        event_bytes: Counter = match (&self.output, &self.source) {
            (None, None) => register_counter!("component_sent_event_bytes_total"),
            (Some(output), None) => register_counter!("component_sent_event_bytes_total", "output" => output.clone()),
            (None, Some(source)) => register_counter!("component_sent_event_bytes_total", "source" => source.clone()),
            (Some(output), Some(source)) => register_counter!("component_sent_event_bytes_total", "output" => output.clone(), "source" => source.clone()),
        },

        output: Option<SharedString> = self.output,

        source: Option<SharedString> = self.source,
    }

    fn emit(&self, data: CountByteSize) {
        let CountByteSize(count, byte_size) = data;

        match (&self.output, &self.source) {
            (None, None) => trace!(message = "Events sent.", count = %count, byte_size = %byte_size),
            (Some(output), None) => trace!(message = "Events sent.", count = %count, byte_size = %byte_size, output = %output),
            (None, Some(source)) => trace!(message = "Events sent.", count = %count, byte_size = %byte_size, source = %source),
            (Some(output), Some(source)) => trace!(message = "Events sent.", count = %count, byte_size = %byte_size, output = %output, source = %source),
        }

        self.events.increment(count as u64);
        self.events_out.increment(count as u64);
        self.event_bytes.increment(byte_size as u64);
    }
);

impl EventsSent {
    #[must_use]
    pub fn sources_matrix(
        sources: Vec<ComponentKey>,
        output: Option<SharedString>,
    ) -> HashMap<Option<usize>, EventsSentHandle> {
        sources
            .into_iter()
            .enumerate()
            .map({ 
                let output = output.clone();

                move |(id, key)| {
                let handle = register(Self::from((
                    Output(output.clone()),
                    Source(Some(key.into_id().into())),
                )));

                (Some(id), handle)
            }})
            .chain(std::iter::once((
                None,
                register(Self::from((Output(None), Source(output)))),
            )))
            .collect()
    }
}

impl From<Output> for EventsSent {
    fn from(output: Output) -> Self {
        Self {
            output: output.0,
            source: None,
        }
    }
}

impl From<(Output, Source)> for EventsSent {
    fn from((output, source): (Output, Source)) -> Self {
        Self {
            output: output.0,
            source: source.0,
        }
    }
}
