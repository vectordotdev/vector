use std::num::ParseFloatError;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::emit;
use vector_common::internal_event::{
    error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
};

pub struct LogToMetricFieldNullError<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for LogToMetricFieldNullError<'a> {
    fn emit(self) {
        let reason = "Unable to convert null field.";
        error!(
            message = reason,
            error_code = "field_null",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            null_field = %self.field,
            internal_log_rate_limit = true
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

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}

pub struct LogToMetricParseFloatError<'a> {
    pub field: &'a str,
    pub error: ParseFloatError,
}

impl<'a> InternalEvent for LogToMetricParseFloatError<'a> {
    fn emit(self) {
        let reason = "Failed to parse field as float.";
        error!(
            message = reason,
            error = ?self.error,
            field = %self.field,
            error_code = "failed_parsing_float",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true
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

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}
