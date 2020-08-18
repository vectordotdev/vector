use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct HTTPEventsReceived {
    pub events_count: usize,
    pub byte_size: usize,
}

impl InternalEvent for HTTPEventsReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Sending events.",
            events_count = %self.events_count,
            byte_size = %self.byte_size,
        );
    }

    fn emit_metrics(&self) {
        counter!("events_processed", self.events_count as u64,
            "component_kind" => "source",
            "component_type" => "http",
        );
        counter!("bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "http",
        );
    }
}

#[derive(Debug)]
pub struct HTTPBadRequest<'a> {
    pub error_code: u16,
    pub error_message: &'a str,
}

impl<'a> InternalEvent for HTTPBadRequest<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "received bad request.",
            code = %self.error_code,
            message = %self.error_message,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "http_bad_requests", 1,
            "component_kind" => "source",
            "component_type" => "http",
        );
    }
}
