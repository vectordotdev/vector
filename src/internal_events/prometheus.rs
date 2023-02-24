#[cfg(feature = "sources-prometheus")]
use std::borrow::Cow;

use hyper::StatusCode;
use metrics::counter;
#[cfg(feature = "sources-prometheus")]
use prometheus_parser::ParserError;
use vector_core::internal_event::InternalEvent;

use crate::emit;
use vector_common::internal_event::{
    error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
};

#[cfg(feature = "sources-prometheus")]
#[derive(Debug)]
pub struct PrometheusParseError<'a> {
    pub error: ParserError,
    pub url: http::Uri,
    pub body: Cow<'a, str>,
}

#[cfg(feature = "sources-prometheus")]
impl<'a> InternalEvent for PrometheusParseError<'a> {
    fn emit(self) {
        error!(
            message = "Parsing error.",
            url = %self.url,
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        debug!(
            message = %format!("Failed to parse response:\n\n{}\n\n", self.body),
            url = %self.url,
            internal_log_rate_limit = true
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "url" => self.url.to_string(),
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusRemoteWriteParseError {
    pub error: prost::DecodeError,
}

impl InternalEvent for PrometheusRemoteWriteParseError {
    fn emit(self) {
        error!(
            message = "Could not decode request body.",
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("parse_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusServerRequestComplete {
    pub status_code: StatusCode,
}

impl InternalEvent for PrometheusServerRequestComplete {
    fn emit(self) {
        let message = "Request to prometheus server complete.";
        if self.status_code.is_success() {
            debug!(message, status_code = %self.status_code);
        } else {
            warn!(message, status_code = %self.status_code);
        }
        counter!("requests_received_total", 1);
    }
}

#[derive(Debug)]
pub struct PrometheusNormalizationError;

impl InternalEvent for PrometheusNormalizationError {
    fn emit(self) {
        let normalization_reason = "Prometheus metric normalization failed.";
        error!(
            message = normalization_reason,
            error_type = error_type::CONVERSION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: normalization_reason
        });
    }
}
