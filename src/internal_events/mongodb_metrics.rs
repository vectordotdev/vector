use super::InternalEvent;
use metrics::{counter, histogram};
use mongodb::{bson, error::Error as MongoError};
use std::time::Instant;

#[derive(Debug)]
pub struct MongoDBMetricsCollectCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for MongoDBMetricsCollectCompleted {
    fn emit_logs(&self) {
        debug!(message = "Collect completed.");
    }

    fn emit_metrics(&self) {
        counter!("vector_collect_completed", 1);
        histogram!("vector_collect_duration_nanoseconds", self.end - self.start);
    }
}

pub struct MongoDBMetricsRequestError<'a> {
    pub error: MongoError,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDBMetricsRequestError<'a> {
    fn emit_logs(&self) {
        error!(message = "MongoDB request error.", endpoint = %self.endpoint, error = %self.error)
    }

    fn emit_metrics(&self) {
        counter!("vector_request_error", 1,
            "component_kind" => "source",
            "component_type" => "mongodb_metrics",
        );
    }
}

pub struct MongoDBMetricsBsonParseError<'a> {
    pub error: bson::de::Error,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDBMetricsBsonParseError<'a> {
    fn emit_logs(&self) {
        error!(message = "BSON document parse error.", endpoint = %self.endpoint, error = ?self.error)
    }

    fn emit_metrics(&self) {
        counter!("vector_bson_parse_error", 1,
            "component_kind" => "source",
            "component_type" => "mongodb_metrics",
        )
    }
}
