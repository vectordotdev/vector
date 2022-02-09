use std::num::ParseFloatError;

use super::prelude::error_stage;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::template::TemplateParseError;

pub struct LogToMetricFieldNullError<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for LogToMetricFieldNullError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Field is null.",
            error = "Unable to convert null field",
            error_type = "condition_failed",
            stage = error_stage::PROCESSING,
            null_field = %self.field,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Unable to convert null field",
            "error_type" => "condition_failed",
            "stage" => error_stage::PROCESSING,
            "null_field" => self.field.to_string(),
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "field_null",
        );
    }
}

pub struct LogToMetricParseFloatError<'a> {
    pub field: &'a str,
    pub error: ParseFloatError,
}

impl<'a> InternalEvent for LogToMetricParseFloatError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to parse field as float.",
            field = %self.field,
            error = %self.error,
            error_type = "parser_failed",
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "parser_failed",
            "stage" => error_stage::PROCESSING,
            "field" => self.field.to_string(),
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "parse_error",
        );
    }
}

pub struct LogToMetricTemplateParseError {
    pub error: TemplateParseError,
}

impl InternalEvent for LogToMetricTemplateParseError {
    fn emit_logs(&self) {
        error!(
            message = "Failed to parse template.",
            error = ?self.error,
            error_type = "template_failed",
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "template_failed",
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "template_error",
        );
    }
}
