use std::time::Instant;

use metrics::{counter, histogram};
pub use vector_lib::internal_event::EventsReceived;
use vector_lib::internal_event::InternalEvent;
use vector_lib::internal_event::{error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL};

#[derive(Debug)]
pub struct EndpointBytesReceived<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
    pub endpoint: &'a str,
}

impl InternalEvent for EndpointBytesReceived<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint,
        );
        counter!(
            "component_received_bytes_total",
            "protocol" => self.protocol.to_owned(),
            "endpoint" => self.endpoint.to_owned(),
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct EndpointBytesSent<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for EndpointBytesSent<'a> {
    fn emit(self) {
        trace!(
            message = "Bytes sent.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint
        );
        counter!(
            "component_sent_bytes_total",
            "protocol" => self.protocol.to_string(),
            "endpoint" => self.endpoint.to_string()
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct SocketOutgoingConnectionError<E> {
    pub error: E,
}

impl<E: std::error::Error> InternalEvent for SocketOutgoingConnectionError<E> {
    fn emit(self) {
        error!(
            message = "Unable to connect.",
            error = %self.error,
            error_code = "failed_connecting",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => "failed_connecting",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}

const STREAM_CLOSED: &str = "stream_closed";

#[derive(Debug)]
pub struct StreamClosedError {
    pub count: usize,
}

impl InternalEvent for StreamClosedError {
    fn emit(self) {
        error!(
            message = "Failed to forward event(s), downstream is closed.",
            error_code = STREAM_CLOSED,
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_code" => STREAM_CLOSED,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason: "Downstream is closed.",
        });
    }
}

#[derive(Debug)]
pub struct CollectionCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for CollectionCompleted {
    fn emit(self) {
        debug!(message = "Collection completed.");
        counter!("collect_completed_total").increment(1);
        histogram!("collect_duration_seconds").record(self.end - self.start);
    }
}

#[derive(Debug)]
pub struct SinkRequestBuildError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for SinkRequestBuildError<E> {
    fn emit(self) {
        // Providing the name of the sink with the build error is not necessary because the emitted log
        // message contains the sink name in `component_type` field thanks to `tracing` spans. For example:
        // "<timestamp> ERROR sink{component_kind="sink" component_id=sink0 component_type=aws_s3 component_name=sink0}: vector::internal_events::common: Failed to build request."
        error!(
            message = format!("Failed to build request."),
            error = %self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
