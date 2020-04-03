use super::InternalEvent;
use crate::sources::prometheus::parser::ParserError;
use metrics::counter;

pub struct PrometheusRequestCompleted;

impl InternalEvent for PrometheusRequestCompleted {
    fn emit_metrics(&self) {
        counter!("requests_completed", 1,
            "component_kind" => "source",
            "component_type" => "prometheus",
        );
    }
}

pub struct PrometheusParseError {
    pub error: ParserError,
}

impl InternalEvent for PrometheusParseError {
    fn emit_logs(&self) {
        error!(message = "parsing error", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("parse_errors", 1,
            "component_kind" => "source",
            "component_type" => "prometheus",
        );
    }
}

pub struct PrometheusHttpError {
    pub error: hyper::Error,
}

impl InternalEvent for PrometheusHttpError {
    fn emit_logs(&self) {
        error!(message = "http request processing error", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("http_request_errors", 1,
            "component_kind" => "source",
            "component_type" => "prometheus",
        );
    }
}
