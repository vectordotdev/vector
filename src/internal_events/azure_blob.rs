use super::prelude::error_stage;
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
            message = %format!("HTTP error response: {}", self.code.canonical_reason().unwrap_or_else(|| self.code.as_str())),
            error_code = %format!("http_response_{}", self.code.as_u16()),
            error_type = "request_failed",
            stage = error_stage::SENDING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => format!("http_response_{}", self.code.as_u16()),
            "error_type" => "request_failed",
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
            error_code = "error_processing_request",
            error_type = "request_failed",
            stage = error_stage::SENDING,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "error_processing_request",
            "error_type" => "request_failed",
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
