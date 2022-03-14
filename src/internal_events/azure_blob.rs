use super::prelude::{error_stage, error_type, http_error_code};
use metrics::counter;
use uuid::Uuid;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AzureBlobErrorResponse {
    pub code: hyper::StatusCode,
}

impl InternalEvent for AzureBlobErrorResponse {
    fn emit_logs(&self) {
        error!(
            message = "HTTP error response",
            error_code = %http_error_code(self.code.as_u16()),
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => http_error_code(self.code.as_u16()),
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
        // deprecated
        counter!("http_error_response_total", 1);
    }
}

#[derive(Debug)]
pub struct AzureBlobHttpError {
    pub error: String,
}

impl InternalEvent for AzureBlobHttpError {
    fn emit_logs(&self) {
        error!(
            message = "Error processing request.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
        // deprecated
        counter!("http_request_errors_total", 1);
    }
}

pub struct AzureBlobEventsSent {
    pub request_id: Uuid,
    pub byte_size: usize,
}

impl InternalEvent for AzureBlobEventsSent {
    fn emit_logs(&self) {
        trace!(message = "Events sent.", request_id = %self.request_id);
    }

    fn emit_metrics(&self) {
        counter!("component_sent_events_total", 1);
        counter!("component_sent_event_bytes_total", self.byte_size as u64);
        // deprecated
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}
