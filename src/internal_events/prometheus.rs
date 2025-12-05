#![allow(dead_code)] // TODO requires optional feature compilation

#[cfg(feature = "sources-prometheus-scrape")]
use std::borrow::Cow;

use metrics::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::{
    ComponentEventsDropped, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};
#[cfg(feature = "sources-prometheus-scrape")]
use vector_lib::prometheus::parser::ParserError;

#[cfg(feature = "sources-prometheus-scrape")]
#[derive(Debug, NamedInternalEvent)]
pub struct PrometheusParseError<'a> {
    pub error: ParserError,
    pub url: http::Uri,
    pub body: Cow<'a, str>,
}

#[cfg(feature = "sources-prometheus-scrape")]
impl InternalEvent for PrometheusParseError<'_> {
    fn emit(self) {
        error!(
            message = "Parsing error.",
            url = %self.url,
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        debug!(
            message = %format!("Failed to parse response:\n\n{}\n\n", self.body),
            url = %self.url
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
            "url" => self.url.to_string(),
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct PrometheusNormalizationError;

impl InternalEvent for PrometheusNormalizationError {
    fn emit(self) {
        let normalization_reason = "Prometheus metric normalization failed.";
        error!(
            message = normalization_reason,
            error_type = error_type::CONVERSION_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::CONVERSION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: normalization_reason
        });
    }
}
