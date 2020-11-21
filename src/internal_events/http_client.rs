use super::InternalEvent;
use http::{Request, Response};
use hyper::{body::HttpBody, Error};
use metrics::{counter, histogram};
use std::time::Duration;

#[derive(Debug)]
pub struct AboutToSendHTTPRequest<'a, T> {
    pub request: &'a Request<T>,
}

impl<'a, T: HttpBody> InternalEvent for AboutToSendHTTPRequest<'a, T> {
    fn emit_logs(&self) {
        debug!(
            message = "Sending HTTP request.",
            uri = %self.request.uri(),
            method = %self.request.method(),
            version = ?self.request.version(),
            headers = ?self.request.headers(),
            body = %FormatBody(self.request.body()),
        );
    }

    fn emit_metrics(&self) {
        counter!("http_client_requests_sent_total", 1, "method" => self.request.method().to_string());
    }
}

#[derive(Debug)]
pub struct GotHTTPResponse<'a, T> {
    pub response: &'a Response<T>,
    pub roundtrip: Duration,
}

impl<'a, T: HttpBody> InternalEvent for GotHTTPResponse<'a, T> {
    fn emit_logs(&self) {
        debug!(
            message = "HTTP response.",
            status = %self.response.status(),
            version = ?self.response.version(),
            headers = ?self.response.headers(),
            body = %FormatBody(self.response.body()),
        );
    }

    fn emit_metrics(&self) {
        counter!("http_client_responses_total", 1, "status" => self.response.status().to_string());
        histogram!("http_client_rtt_ns", self.roundtrip);
        histogram!("http_client_response_rtt_ns", self.roundtrip, "status" => self.response.status().to_string());
    }
}

#[derive(Debug)]
pub struct GotHTTPError<'a> {
    pub error: &'a Error,
    pub roundtrip: Duration,
}

impl<'a> InternalEvent for GotHTTPError<'a> {
    fn emit_logs(&self) {
        debug!(
            message = "HTTP error.",
            error = %self.error,
        );
    }

    fn emit_metrics(&self) {
        counter!("http_client_errors_total", 1, "error_kind" => self.error.to_string());
        histogram!("http_client_rtt_ns", self.roundtrip);
        histogram!("http_client_error_rtt_ns", self.roundtrip, "error_kind" => self.error.to_string());
    }
}

/// Newtype placeholder to provide a formatter for the request and response body.
struct FormatBody<'a, B>(&'a B);

impl<'a, B: HttpBody> std::fmt::Display for FormatBody<'a, B> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let size = self.0.size_hint();
        match (size.lower(), size.upper()) {
            (0, None) => write!(fmt, "[unknown]"),
            (lower, None) => write!(fmt, "[>={} bytes]", lower),

            (0, Some(0)) => write!(fmt, "[empty]"),
            (0, Some(upper)) => write!(fmt, "[<={} bytes]", upper),

            (lower, Some(upper)) if lower == upper => write!(fmt, "[{} bytes]", lower),
            (lower, Some(upper)) => write!(fmt, "[{}..={} bytes]", lower, upper),
        }
    }
}
