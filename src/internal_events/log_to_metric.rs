use std::num::ParseFloatError;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};
use crate::template::TemplateParseError;

pub struct LogToMetricFieldNullError<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for LogToMetricFieldNullError<'a> {
    fn emit(self) {
        error!(
            message = "Unable to convert null field.",
            error_code = "field_null",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            null_field = %self.field,
            internal_log_rate_secs = 30
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "field_null",
            "error_type" => error_type::CONDITION_FAILED,
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
    fn emit(self) {
        error!(
            message = "Failed to parse field as float.",
            error = ?self.error,
            field = %self.field,
            error_code = "failed_parsing_float",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_parsing_float",
            "error_type" => error_type::PARSER_FAILED,
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
    fn emit(self) {
        error!(
            message = "Failed to parse template.",
            error = ?self.error,
            error_code = "failed_parsing_template",
            error_type = error_type::TEMPLATE_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 30,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_parsing_template",
            "error_type" => error_type::TEMPLATE_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "template_error",
        );
    }
}
