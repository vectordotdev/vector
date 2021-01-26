use super::InternalEvent;
use crate::template::TemplateRenderingError;
use metrics::counter;

#[derive(Debug)]
pub struct AwsSqsEventSent<'a> {
    pub byte_size: usize,
    pub message_id: Option<&'a String>,
}

impl InternalEvent for AwsSqsEventSent<'_> {
    fn emit_logs(&self) {
        trace!(message = "Event sent.", message_id = ?self.message_id);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct AwsSqsTemplateRenderingError {
    pub error: TemplateRenderingError,
}

impl<'a> InternalEvent for AwsSqsTemplateRenderingError {
    fn emit_logs(&self) {
        warn!(
            message = "Failed to render template; dropping event.",
            error = %self.error,
            internal_log_rate_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!("template_rendering_errors_total", 1);
    }
}
