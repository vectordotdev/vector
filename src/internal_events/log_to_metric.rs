use std::num::ParseFloatError;

use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL};

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

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}

//  Metric Metadata Events and Errors
pub struct MetricMetadataInvalidFieldValueError<'a> {
    pub field: &'a str,
    pub field_value: &'a str,
}

impl<'a> InternalEvent for MetricMetadataInvalidFieldValueError<'a> {
    fn emit(self) {
        let reason = "Field contained unsupported value.";
        error!(
            message = reason,
            field = %self.field,
            field_value = %self.field_value,
            error_code = "failed_parsing_float",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "invalid_field_value",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "field" => self.field.to_string(),
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}

pub struct MetricMetadataParseError<'a> {
    pub field: &'a str,
    pub kind: &'a str,
}

impl<'a> InternalEvent for MetricMetadataParseError<'a> {
    fn emit(self) {
        let reason = "Failed to parse field as float.";
        error!(
            message = reason,
            field = %self.field,
            error_code = format!("failed_parsing_{}", self.kind),
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => format!("failed_parsing_{}", self.kind),
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "field" => self.field.to_string(),
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}

pub struct MetricMetadataMetricDetailsNotFoundError {}

impl InternalEvent for MetricMetadataMetricDetailsNotFoundError {
    fn emit(self) {
        let reason = "Missing required metric details. Required one of gauge, distribution, histogram, summary, counter";
        error!(
            message = reason,
            error_code = "missing_metric_details",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "missing_metric_details",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}
