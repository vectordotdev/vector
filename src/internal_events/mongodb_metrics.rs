use metrics::counter;
use mongodb::{bson, error::Error as MongoError};
use vector_lib::{
    NamedInternalEvent,
    internal_event::{InternalEvent, error_stage, error_type},
    json_size::JsonSize,
};

#[derive(Debug, NamedInternalEvent)]
pub struct MongoDbMetricsEventsReceived<'a> {
    pub count: usize,
    pub byte_size: JsonSize,
    pub endpoint: &'a str,
}

impl InternalEvent for MongoDbMetricsEventsReceived<'_> {
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

#[derive(NamedInternalEvent)]
pub struct MongoDbMetricsRequestError<'a> {
    pub error: MongoError,
    pub endpoint: &'a str,
}

impl InternalEvent for MongoDbMetricsRequestError<'_> {
    fn emit(self) {
        error!(
            message = "MongoDb request error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(NamedInternalEvent)]
pub struct MongoDbMetricsBsonParseError<'a> {
    pub error: bson::de::Error,
    pub endpoint: &'a str,
}

impl InternalEvent for MongoDbMetricsBsonParseError<'_> {
    fn emit(self) {
        error!(
            message = "BSON document parse error.",
            endpoint = %self.endpoint,
            error = ?self.error,
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::RECEIVING,
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
