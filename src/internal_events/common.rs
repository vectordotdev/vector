use std::time::Instant;

use vector_lib::NamedInternalEvent;
pub use vector_lib::internal_event::EventsReceived;
use vector_lib::internal_event::{
    ComponentEventsDropped, CounterName, HistogramName, INTENTIONAL, InternalEvent, UNINTENTIONAL,
    error_stage, error_type,
};
use vector_lib::{counter, histogram};

#[derive(Debug, NamedInternalEvent)]
pub struct KeyOutsideBasePrefixError<'a> {
    /// Bounded preview of the rejected key — never the full rendered value,
    /// so attacker-controlled input can't amplify into logs and secrets in
    /// templated header/query fields don't leak.
    pub key_preview: &'a str,
    /// Full byte length of the rejected rendered value.
    pub key_len: usize,
    pub message: &'a str,
}

impl InternalEvent for KeyOutsideBasePrefixError<'_> {
    fn emit(self) {
        error!(
            message = "Rendered key is outside the configured base prefix; dropping event.",
            key_preview = self.key_preview,
            key_len = self.key_len,
            error = self.message,
            error_type = error_type::CONFINEMENT_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::CONFINEMENT_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: 1,
            reason: "Rendered key outside base prefix.",
        });
    }
}

#[derive(Debug, NamedInternalEvent)]
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
            CounterName::ComponentReceivedBytesTotal,
            "protocol" => self.protocol.to_owned(),
            "endpoint" => self.endpoint.to_owned(),
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct EndpointBytesSent<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
    pub endpoint: &'a str,
}

impl InternalEvent for EndpointBytesSent<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes sent.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint
        );
        counter!(
            CounterName::ComponentSentBytesTotal,
            "protocol" => self.protocol.to_string(),
            "endpoint" => self.endpoint.to_string()
        )
        .increment(self.byte_size as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "failed_connecting",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
    }
}

const STREAM_CLOSED: &str = "stream_closed";

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
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

#[derive(Debug, NamedInternalEvent)]
pub struct CollectionCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for CollectionCompleted {
    fn emit(self) {
        debug!(message = "Collection completed.");
        counter!(CounterName::CollectCompletedTotal).increment(1);
        histogram!(HistogramName::CollectDurationSeconds).record(self.end - self.start);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
