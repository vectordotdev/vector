use metrics::counter;
use mongodb::{bson, error::Error as MongoError};
use vector_lib::internal_event::InternalEvent;
use vector_lib::{
    internal_event::{error_stage, error_type},
    json_size::JsonSize,
};

#[derive(Debug)]
pub struct MongoDbMetricsEventsReceived<'a> {
    pub count: usize,
    pub byte_size: JsonSize,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDbMetricsEventsReceived<'a> {
    // ## skip check-duplicate-events ##
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = self.count,
            byte_size = self.byte_size.get(),
            endpoint = self.endpoint,
        );
        counter!(
            "component_received_events_total",
            "endpoint" => self.endpoint.to_owned(),
        )
        .increment(self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            "endpoint" => self.endpoint.to_owned(),
        )
        .increment(self.byte_size.get() as u64);
    }
}

pub struct MongoDbMetricsRequestError<'a> {
    pub error: MongoError,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDbMetricsRequestError<'a> {
    fn emit(self) {
        error!(
            message = "MongoDb request error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

pub struct MongoDbMetricsBsonParseError<'a> {
    pub error: bson::de::Error,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for MongoDbMetricsBsonParseError<'a> {
    fn emit(self) {
        error!(
            message = "BSON document parse error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::RECEIVING,
            "endpoint" => self.endpoint.to_owned(),
        )
        .increment(1);
    }
}
