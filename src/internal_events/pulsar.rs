use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct PulsarEncodeEventFailed<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for PulsarEncodeEventFailed<'a> {
    fn emit_logs(&self) {
        debug!(message = "Event encode failed.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("encode_errors_total", 1);
    }
}
