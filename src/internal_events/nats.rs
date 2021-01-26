use super::InternalEvent;
use crate::template::TemplateRenderingError;
use metrics::counter;
use std::io::Error;

#[derive(Debug)]
pub struct NatsEventSendSuccess {
    pub byte_size: usize,
}

impl InternalEvent for NatsEventSendSuccess {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct NatsEventSendFail {
    pub error: Error,
}

impl InternalEvent for NatsEventSendFail {
    fn emit_logs(&self) {
        error!(message = "Failed to send message.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("send_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct NatsTemplateRenderingError {
    pub error: TemplateRenderingError,
}

impl InternalEvent for NatsTemplateRenderingError {
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
