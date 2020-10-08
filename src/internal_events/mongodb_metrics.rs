use super::InternalEvent;
use metrics::counter;
use mongodb::{bson::document::ValueAccessError, error::Error as MongoError};

pub struct MongoDBMetricsRequestError<'a> {
    pub error: MongoError,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDBMetricsRequestError<'a> {
    fn emit_logs(&self) {
        error!(message = "MongoDB request error.", endpoint = %self.endpoint, error = %self.error)
    }

    fn emit_metrics(&self) {
        counter!("request_error", 1,
            "component_kind" => "source",
            "component_type" => "mongodb_metrics",
        );
    }
}

pub struct MongoDBMetricsBsonParseError<'a> {
    pub error: ValueAccessError,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDBMetricsBsonParseError<'a> {
    fn emit_logs(&self) {
        error!(message = "BSON document parse error.", endpoint = %self.endpoint, error = %self.error)
    }

    fn emit_metrics(&self) {
        counter!("bson_parse_error", 1,
            "component_kind" => "source",
            "component_type" => "mongodb_metrics",
        )
    }
}
