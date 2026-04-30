use std::{error::Error, time::Duration};

use http::Response;
use vector_lib::{
    NamedInternalEvent,
    internal_event::{InternalEvent, MetricName, error_stage, error_type},
    json_size::JsonSize,
};
use vector_lib::{counter, histogram};

const HTTP_STATUS_LABEL: &str = "status";

#[derive(Debug, NamedInternalEvent)]
pub struct HttpServerRequestReceived;

impl InternalEvent for HttpServerRequestReceived {
    fn emit(self) {
        debug!(message = "Received HTTP request.");
        counter!(MetricName::HttpServerRequestsReceivedTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct HttpServerResponseSent<'a, B> {
    pub response: &'a Response<B>,
    pub latency: Duration,
}

impl<B> InternalEvent for HttpServerResponseSent<'_, B> {
    fn emit(self) {
        let labels = &[(
            HTTP_STATUS_LABEL,
            self.response.status().as_u16().to_string(),
        )];
        counter!(MetricName::HttpServerResponsesSentTotal, labels).increment(1);
        histogram!(MetricName::HttpServerHandlerDurationSeconds, labels).record(self.latency);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
            MetricName::ComponentReceivedBytesTotal,
            "http_path" => self.http_path.to_string(),
            "protocol" => self.protocol,
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
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

        histogram!(MetricName::ComponentReceivedEventsCount).record(self.count as f64);
        counter!(
            MetricName::ComponentReceivedEventsTotal,
            "http_path" => self.http_path.to_string(),
            "protocol" => self.protocol,
        )
        .increment(self.count as u64);
        counter!(
            MetricName::ComponentReceivedEventBytesTotal,
            "http_path" => self.http_path.to_string(),
            "protocol" => self.protocol,
        )
        .increment(self.byte_size.get() as u64);
    }
}

#[cfg(feature = "sources-utils-http")]
#[derive(Debug, NamedInternalEvent)]
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

#[cfg(feature = "sources-utils-http")]
impl InternalEvent for HttpBadRequest<'_> {
    fn emit(self) {
        warn!(
            message = "Received bad request.",
            error = %self.message,
            error_code = %self.error_code,
            error_type = error_type::REQUEST_FAILED,
            error_stage = error_stage::RECEIVING,
            http_code = %self.code,
        );
        counter!(
            MetricName::ComponentErrorsTotal,
            "error_code" => self.error_code,
            "error_type" => error_type::REQUEST_FAILED,
            "error_stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct HttpDecompressError<'a> {
    pub error: &'a dyn Error,
    pub encoding: &'a str,
}

impl InternalEvent for HttpDecompressError<'_> {
    fn emit(self) {
        error!(
            message = "Failed decompressing payload.",
            error = %self.error,
            error_code = "failed_decompressing_payload",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::RECEIVING,
            encoding = %self.encoding
        );
        counter!(
            MetricName::ComponentErrorsTotal,
            "error_code" => "failed_decompressing_payload",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(NamedInternalEvent)]
pub struct HttpInternalError<'a> {
    pub message: &'a str,
}

impl InternalEvent for HttpInternalError<'_> {
    fn emit(self) {
        error!(
            message = %self.message,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING
        );
        counter!(
            MetricName::ComponentErrorsTotal,
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}
