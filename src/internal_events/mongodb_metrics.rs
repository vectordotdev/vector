// ## skip check-events ##

use std::time::Instant;

use metrics::{counter, histogram};
use mongodb::{bson, error::Error as MongoError};
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct MongoDbMetricsEventsReceived<'a> {
    pub count: usize,
    pub uri: &'a str,
}

impl<'a> InternalEvent for MongoDbMetricsEventsReceived<'a> {
    fn emit_metrics(&self) {
        counter!(
            "component_received_events_total", self.count as u64,
            "uri" => self.uri.to_owned(),
        );
        counter!(
            "events_in_total", self.count as u64,
            "uri" => self.uri.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct MongoDbMetricsCollectCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for MongoDbMetricsCollectCompleted {
    fn emit_logs(&self) {
        debug!(message = "Collection completed.");
    }

    fn emit_metrics(&self) {
        counter!("collect_completed_total", 1);
        histogram!("collect_duration_seconds", self.end - self.start);
    }
}

pub struct MongoDbMetricsRequestError<'a> {
    pub error: MongoError,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDbMetricsRequestError<'a> {
    fn emit_logs(&self) {
        error!(message = "MongoDb request error.", endpoint = %self.endpoint, error = ?self.error)
    }

    fn emit_metrics(&self) {
        counter!("request_errors_total", 1);
    }
}

pub struct MongoDbMetricsBsonParseError<'a> {
    pub error: bson::de::Error,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDbMetricsBsonParseError<'a> {
    fn emit_logs(&self) {
        error!(message = "BSON document parse error.", endpoint = %self.endpoint, error = ?self.error)
    }

    fn emit_metrics(&self) {
        counter!("parse_errors_total", 1);
    }
}
