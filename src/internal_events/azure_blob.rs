use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct AzureBlobErrorResponse {
    pub code: hyper::StatusCode,
}

impl InternalEvent for AzureBlobErrorResponse {
    fn emit_logs(&self) {
        error!(message = "HTTP error response.", code = %self.code);
    }

    fn emit_metrics(&self) {
        counter!("http_error_response_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct AzureBlobHttpError {
    pub error: String,
}

impl InternalEvent for AzureBlobHttpError {
    fn emit_logs(&self) {
        error!(
            message = "Error processing request.",
            error = ?self.error,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors_total", 1);
    }
}
