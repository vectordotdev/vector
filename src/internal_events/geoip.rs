use super::prelude::error_stage;
use metrics::counter;
use std::net::AddrParseError;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct GeoipIpAddressParseError<'a> {
    pub error: AddrParseError,
    pub address: &'a str,
}

impl<'a> InternalEvent for GeoipIpAddressParseError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "IP Address not parsed correctly.",
            error = %self.error,
            error_type = "parser_failed",
            stage = error_stage::PROCESSING,
            address = %self.address,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => self.error.to_string(),
            "error_type" => "parser_failed",
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "type_ip_address_parse_error",
        );
    }
}
