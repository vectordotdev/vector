use metrics::counter;
use vector_lib::internal_event::InternalEvent;
use vector_lib::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};

use super::prelude::http_error_code;

#[derive(Debug)]
pub struct HttpClientEventsReceived {
    pub byte_size: JsonSize,
    pub count: usize,
    pub url: String,
}

impl InternalEvent for HttpClientEventsReceived {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            url = %self.url,
        );
        counter!(
            "component_received_events_total",
            "uri" => self.url.clone(),
        )
        .increment(self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            "uri" => self.url.clone(),
        )
        .increment(self.byte_size.get() as u64);
    }
}

#[derive(Debug)]
pub struct HttpClientHttpResponseError {
    pub code: hyper::StatusCode,
    pub url: String,
}

impl InternalEvent for HttpClientHttpResponseError {
    fn emit(self) {
        error!(
            message = "HTTP error response.",
            url = %self.url,
            stage = error_stage::RECEIVING,
            error_type = error_type::REQUEST_FAILED,
            error_code = %http_error_code(self.code.as_u16()),
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "url" => self.url,
            "stage" => error_stage::RECEIVING,
            "error_type" => error_type::REQUEST_FAILED,
            "error_code" => http_error_code(self.code.as_u16()),
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct HttpClientHttpError {
    pub error: crate::Error,
    pub url: String,
}

impl InternalEvent for HttpClientHttpError {
    fn emit(self) {
        error!(
            message = "HTTP request processing error.",
            url = %self.url,
            error = ?self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "url" => self.url,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}
