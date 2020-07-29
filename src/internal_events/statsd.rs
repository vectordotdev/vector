use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct StatsdEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for StatsdEventReceived {
    fn emit_logs(&self) {
        trace!(message = "received line.", byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!(
            "events_processed", 1,
            "component_kind" => "source",
            "component_name" => "statsd",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_name" => "statsd",
        );
    }
}

#[derive(Debug)]
pub struct StatsdInvalidRecord<'a> {
    pub error: crate::sources::statsd::parser::ParseError,
    pub text: &'a str,
}

impl InternalEvent for StatsdInvalidRecord<'_> {
    fn emit_logs(&self) {
        error!(message = "Invalid record from statsd, discarding.", error = %self.error, text = %self.text);
    }

    fn emit_metrics(&self) {
        counter!("invalid_record", 1,
                 "component_kind" => "source",
                 "component_name" => "statsd",
        );
        counter!("invalid_record_bytes", self.text.len() as u64,
                 "component_kind" => "source",
                 "component_name" => "statsd",
        );
    }
}
