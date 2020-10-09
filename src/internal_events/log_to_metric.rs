use super::InternalEvent;
use metrics::counter;

pub(crate) struct LogToMetricEventProcessed;

impl InternalEvent for LogToMetricEventProcessed {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.");
    }

    fn emit_metrics(&self) {
        counter!("events_processed", 1);
    }
}

pub(crate) struct LogToMetricFieldNotFound;

impl InternalEvent for LogToMetricFieldNotFound {
    fn emit_logs(&self) {
        warn!(message = "Field not found.", rate_limit_sec = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
                 "error_type" => "field_not_found",
        );
    }
}

pub(crate) struct LogToMetricParseError<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for LogToMetricParseError<'a> {
    fn emit_logs(&self) {
        warn!(message = "Failed to parse.", error = %self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
                 "error_type" => "parse_error",
        );
    }
}

pub(crate) struct LogToMetricRenderError {
    pub error: String,
}

impl InternalEvent for LogToMetricRenderError {
    fn emit_logs(&self) {
        warn!(message = "Unable to render.", error = %self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
                 "error_type" => "render_error",
        );
    }
}

pub(crate) struct LogToMetricTemplateError {
    pub error: crate::template::TemplateError,
}

impl InternalEvent for LogToMetricTemplateError {
    fn emit_logs(&self) {
        warn!(message = "Failed to parse.", error = %self.error, rate_limit_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors", 1,
                 "error_type" => "template_error",
        );
    }
}
