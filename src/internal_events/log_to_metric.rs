// ## skip check-events ##

use std::num::ParseFloatError;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::template::TemplateParseError;

pub struct LogToMetricFieldNull<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for LogToMetricFieldNull<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Field is null.",
            null_field = %self.field,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
                 "error_type" => "field_null",
        );
    }
}

pub struct LogToMetricFieldNotFound<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for LogToMetricFieldNotFound<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Field not found.",
            missing_field = %self.field,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
                 "error_type" => "field_not_found",
        );
    }
}

pub struct LogToMetricParseFloatError<'a> {
    pub field: &'a str,
    pub error: ParseFloatError,
}

impl<'a> InternalEvent for LogToMetricParseFloatError<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Failed to parse field as float.",
            field = %self.field,
            error = %self.error,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
                 "error_type" => "parse_error",
        );
    }
}

pub struct LogToMetricTemplateParseError {
    pub error: TemplateParseError,
}

impl InternalEvent for LogToMetricTemplateParseError {
    fn emit_logs(&self) {
        warn!(message = "Failed to parse template.", error = ?self.error, internal_log_rate_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1,
                 "error_type" => "template_error",
        );
    }
}
