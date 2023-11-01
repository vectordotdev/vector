use metrics::counter;
use vector_lib::internal_event::InternalEvent;

use crate::emit;
use vector_lib::internal_event::{
    error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
};

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

pub struct MetricMetadataParseFloatError<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for MetricMetadataParseFloatError<'a> {
    fn emit(self) {
        let reason = "Failed to parse field as float.";
        error!(
            message = reason,
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

pub struct MetricMetadataParseIntError<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for MetricMetadataParseIntError<'a> {
    fn emit(self) {
        let reason = "Failed to parse field as int.";
        error!(
            message = reason,
            field = %self.field,
            error_code = "failed_parsing_int",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_parsing_int",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "field" => self.field.to_string(),
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}

pub struct MetricMetadataParseArrayError<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for MetricMetadataParseArrayError<'a> {
    fn emit(self) {
        let reason = "Failed to parse field as array.";
        error!(
            message = reason,
            field = %self.field,
            error_code = "failed_parsing_array",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_parsing_array",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "field" => self.field.to_string(),
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason })
    }
}
pub struct MetricMetadataMetricDetailsNotFoundError {
}

impl<'a> InternalEvent for MetricMetadataMetricDetailsNotFoundError {
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
