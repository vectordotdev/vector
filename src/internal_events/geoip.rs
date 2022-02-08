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
            error = "Invalid IP address",
            error_type = "parser_failed",
            stage = "processing",
            address = %self.address,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error" => "Invalid IP address",
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
