use super::prelude::{error_stage, error_type};
use metrics::counter;
use std::net::AddrParseError;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct GeoipIpAddressParseError<'a> {
    pub(crate) error: AddrParseError,
    pub address: &'a str,
}

impl<'a> InternalEvent for GeoipIpAddressParseError<'a> {
    fn emit(self) {
        error!(
            message = %format!("IP Address not parsed correctly: {:?}", self.error),
            error_code = "invalid_ip_address",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            address = %self.address,
            internal_log_rate_secs = 30
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "invalid_ip_address",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "address" => self.address.to_string(),
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "type_ip_address_parse_error",
        );
    }
}
