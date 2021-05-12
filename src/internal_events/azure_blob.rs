use super::InternalEvent;
use azure_sdk_core::errors::AzureError;
use metrics::counter;

#[derive(Debug)]
pub struct AzureBlobErrorResponse {
    pub code: hyper::StatusCode,
    pub url: String,
}

impl InternalEvent for AzureBlobErrorResponse {
    fn emit_logs(&self) {
        error!(message = "HTTP error response.", url = %self.url, code = %self.code);
    }

    fn emit_metrics(&self) {
        counter!("http_error_response_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct AzureBlobHttpError<'a> {
    pub error: &'a AzureError,
}

impl InternalEvent for AzureBlobHttpError<'_> {
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
