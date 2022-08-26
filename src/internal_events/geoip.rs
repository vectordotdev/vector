use std::net::AddrParseError;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

use super::prelude::{error_stage, error_type};

#[derive(Debug)]
pub struct GeoipIpAddressParseError<'a> {
    pub(crate) error: AddrParseError,
    pub address: &'a str,
}

impl<'a> InternalEvent for GeoipIpAddressParseError<'a> {
    fn emit(self) {
        error!(
            message = "Events dropped.",
            count = 1,
            error_code = "invalid_ip_address",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            address = %self.address,
            intentional = false,
            reason = %format!("IP Address not parsed correctly: {:?}.", self.error),
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "invalid_ip_address",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "address" => self.address.to_string(),
        );
        counter!(
            "component_discarded_events_total", 1,
            "error_code" => "invalid_ip_address",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "address" => self.address.to_string(),
            "intentional" => "false",
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "type_ip_address_parse_error",
        );
    }
}
