use std::{error::Error, time::Duration};

use http::Response;
use metrics::{counter, histogram};
use vector_lib::internal_event::InternalEvent;
use vector_lib::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};

const HTTP_STATUS_LABEL: &str = "status";

#[derive(Debug)]
pub struct HttpServerRequestReceived;

impl InternalEvent for HttpServerRequestReceived {
    fn emit(self) {
        debug!(
            message = "Received HTTP request.",
            internal_log_rate_limit = true
        );
        counter!("http_server_requests_received_total", 1);
    }
}

#[derive(Debug)]
pub struct HttpServerResponseSent<'a, B> {
    pub response: &'a Response<B>,
    pub latency: Duration,
}

impl<'a, B> InternalEvent for HttpServerResponseSent<'a, B> {
    fn emit(self) {
        let labels = &[(
            HTTP_STATUS_LABEL,
            self.response.status().as_u16().to_string(),
        )];
        counter!("http_server_responses_sent_total", 1, labels);
        histogram!("http_server_handler_duration_seconds", self.latency, labels);
    }
}

#[derive(Debug)]
pub struct HttpBytesReceived<'a> {
    pub byte_size: usize,
    pub http_path: &'a str,
    pub protocol: &'static str,
}

impl InternalEvent for HttpBytesReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            http_path = %self.http_path,
            protocol = %self.protocol
        );
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "http_path" => self.http_path.to_string(),
            "protocol" => self.protocol,
        );
    }
}

#[derive(Debug)]
pub struct HttpEventsReceived<'a> {
    pub count: usize,
    pub byte_size: JsonSize,
    pub http_path: &'a str,
    pub protocol: &'static str,
}

impl InternalEvent for HttpEventsReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
            byte_size = %self.byte_size,
            http_path = %self.http_path,
            protocol = %self.protocol,
        );

        histogram!("component_received_events_count", self.count as f64);
        counter!(
            "component_received_events_total", self.count as u64,
            "http_path" => self.http_path.to_string(),
            "protocol" => self.protocol,
        );
        counter!(
            "component_received_event_bytes_total",
            self.byte_size.get() as u64,
            "http_path" => self.http_path.to_string(),
            "protocol" => self.protocol,
        );
    }
}

#[derive(Debug)]
pub struct HttpBadRequest<'a> {
    code: u16,
    error_code: String,
    message: &'a str,
}

#[cfg(feature = "sources-utils-http")]
impl<'a> HttpBadRequest<'a> {
    pub fn new(code: u16, message: &'a str) -> Self {
        Self {
            code,
            error_code: super::prelude::http_error_code(code),
            message,
        }
    }
}

impl<'a> InternalEvent for HttpBadRequest<'a> {
    fn emit(self) {
        warn!(
            message = "Received bad request.",
            error = %self.message,
            error_code = %self.error_code,
            error_type = error_type::REQUEST_FAILED,
            error_stage = error_stage::RECEIVING,
            http_code = %self.code,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error_code,
            "error_type" => error_type::REQUEST_FAILED,
            "error_stage" => error_stage::RECEIVING,
        );
    }
}

#[derive(Debug)]
pub struct HttpDecompressError<'a> {
    pub error: &'a dyn Error,
    pub encoding: &'a str,
}

impl<'a> InternalEvent for HttpDecompressError<'a> {
    fn emit(self) {
        error!(
            message = "Failed decompressing payload.",
            error = %self.error,
            error_code = "failed_decompressing_payload",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::RECEIVING,
            encoding = %self.encoding,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_decompressing_payload",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}

pub struct HttpInternalError<'a> {
    pub message: &'a str,
}

impl<'a> InternalEvent for HttpInternalError<'a> {
    fn emit(self) {
        error!(
            message = %self.message,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}
