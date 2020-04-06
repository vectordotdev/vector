use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct FileEventReceived<'a> {
    pub file: &'a str,
    pub byte_size: usize,
}

impl InternalEvent for FileEventReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "received one event.",
            %self.file,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "events_received", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
        counter!(
            "bytes_received", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}
