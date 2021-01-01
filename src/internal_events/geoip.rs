use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub(crate) struct GeoipIpAddressParseError<'a> {
    pub address: &'a str,
}

impl<'a> InternalEvent for GeoipIpAddressParseError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "IP Address not parsed correctly.",
            address = %self.address,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "type_ip_address_parse_error");
    }
}

#[derive(Debug)]
pub(crate) struct GeoipFieldDoesNotExist<'a> {
    pub field: &'a str,
}

impl<'a> InternalEvent for GeoipFieldDoesNotExist<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Field does not exist.",
            field = %self.field,
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("processing_errors_total", 1, "error_type" => "type_field_does_not_exist");
    }
}
