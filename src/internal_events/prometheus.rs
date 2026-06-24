#![allow(dead_code)] // TODO requires optional feature compilation

#[cfg(feature = "sources-prometheus-scrape")]
use std::borrow::Cow;

#[cfg(feature = "sources-prometheus-kubernetes-sd")]
use vector_lib::internal_event::GaugeName;
#[cfg(feature = "sources-prometheus-scrape")]
use vector_lib::prometheus::parser::ParserError;
use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{
        ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage, error_type,
    },
};

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
            CounterName::ComponentErrorsTotal,
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
            CounterName::ComponentErrorsTotal,
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
            CounterName::ComponentErrorsTotal,
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

#[cfg(feature = "sources-prometheus-kubernetes-sd")]
#[derive(Debug, NamedInternalEvent)]
pub struct PrometheusKubernetesSdTargetsDiscovered {
    pub count: usize,
}

#[cfg(feature = "sources-prometheus-kubernetes-sd")]
impl InternalEvent for PrometheusKubernetesSdTargetsDiscovered {
    fn emit(self) {
        debug!(
            message = "Prometheus Kubernetes SD discovered targets.",
            count = self.count,
        );
        vector_lib::gauge!(GaugeName::PrometheusKubernetesSdTargetsDiscovered)
            .set(self.count as f64);
    }
}

#[cfg(feature = "sources-prometheus-kubernetes-sd")]
#[derive(Debug, NamedInternalEvent)]
pub struct PrometheusKubernetesSdAnnotationParseError<'a> {
    pub pod: &'a str,
    pub namespace: &'a str,
    pub error: &'a str,
}

#[cfg(feature = "sources-prometheus-kubernetes-sd")]
impl InternalEvent for PrometheusKubernetesSdAnnotationParseError<'_> {
    fn emit(self) {
        warn!(
            message = "Failed to parse prometheus.io annotations on pod.",
            pod = %self.pod,
            namespace = %self.namespace,
            error = %self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
