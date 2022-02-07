// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct GeoipIpAddressParseError<'a> {
    pub address: &'a str,
}

impl<'a> InternalEvent for GeoipIpAddressParseError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "IP Address not parsed correctly.",
            error = "IP Address not parsed correctly.",
            error_type = "parser_failed",
            stage = "processing",
            address = %self.address,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "IP Address not parsed correctly.",
            "error_type" => "parser_failed",
            "stage" => "processing",
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "type_ip_address_parse_error",
        );
    }
}

#[derive(Debug)]
pub struct GeoipFieldDoesNotExistError<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for GeoipFieldDoesNotExistError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Field does not exist.",
            field = %self.field,
            error = "Field does not exist.",
            error_type = "parser_failed",
            stage = "processing",
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Field does not exist.",
            "error_type" => "parser_failed",
            "stage" => "processing",
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "type_field_does_not_exist",
        );
    }
}
