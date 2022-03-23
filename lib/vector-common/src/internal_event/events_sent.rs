use metrics::counter;
use tracing::trace;

use crate::internal_event::InternalEvent;

pub const DEFAULT_OUTPUT: &str = "_default";

#[derive(Debug)]
pub struct EventsSent<'a> {
    pub count: usize,
    pub byte_size: usize,
    pub output: Option<&'a str>,
}

impl<'a> InternalEvent for EventsSent<'a> {
    fn emit(self) {
        if let Some(output) = self.output {
            trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size, output = %output);
        } else {
            trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
        }

        if self.count > 0 {
            if let Some(output) = self.output {
                counter!("component_sent_events_total", self.count as u64, "output" => output.to_owned());
                counter!("events_out_total", self.count as u64, "output" => output.to_owned());
                counter!("component_sent_event_bytes_total", self.byte_size as u64, "output" => output.to_owned());
            } else {
                counter!("component_sent_events_total", self.count as u64);
                counter!("events_out_total", self.count as u64);
                counter!("component_sent_event_bytes_total", self.byte_size as u64);
            }
        }
    }

    fn name(&self) -> Option<&'static str> {
        Some("EventsSent")
    }
}
