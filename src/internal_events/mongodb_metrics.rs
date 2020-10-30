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
        counter!("collect_completed_total", 1);
        histogram!("collect_duration_nanoseconds", self.end - self.start);
    }
}

pub struct MongoDBMetricsRequestError<'a> {
    pub error: MongoError,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDBMetricsRequestError<'a> {
    fn emit_logs(&self) {
        error!(message = "MongoDB request error.", endpoint = %self.endpoint, error = ?self.error)
    }

    fn emit_metrics(&self) {
        counter!("request_error_total", 1);
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
        counter!("bson_parse_error_total", 1);
    }
}
