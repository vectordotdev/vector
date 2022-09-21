use std::time::Instant;

use crate::emit;
use metrics::{counter, histogram};
pub use vector_core::internal_event::EventsReceived;
use vector_core::internal_event::InternalEvent;

use vector_common::internal_event::{error_stage, error_type};

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
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => self.protocol.to_owned(),
            "endpoint" => self.endpoint.to_owned(),
        );
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
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => self.protocol.to_string(),
            "endpoint" => self.endpoint.to_string()
        );
    }
}

const STREAM_CLOSED: &str = "stream_closed";

#[derive(Debug)]
pub struct StreamClosedError {
    pub error: crate::source_sender::ClosedError,
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
            "component_errors_total", 1,
            "error_code" => STREAM_CLOSED,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count as u64,
            reason: "Downstream is closed.",
        });
    }
}

#[derive(Debug)]
pub struct RequestCompleted {
    pub start: Instant,
    pub end: Instant,
}

impl InternalEvent for RequestCompleted {
    fn emit(self) {
        debug!(message = "Request completed.");
        counter!("requests_completed_total", 1);
        histogram!("request_duration_seconds", self.end - self.start);
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
        counter!("collect_completed_total", 1);
        histogram!("collect_duration_seconds", self.end - self.start);
    }
}

#[allow(dead_code)]
pub const INTENTIONAL: bool = true;
pub const UNINTENTIONAL: bool = false;

#[derive(Debug)]
pub struct ComponentEventsDropped<'a, const INTENTIONAL: bool> {
    pub count: u64,
    pub reason: &'a str,
}

impl<'a, const INTENTIONAL: bool> InternalEvent for ComponentEventsDropped<'a, INTENTIONAL> {
    fn emit(self) {
        let message = "Events dropped";
        if INTENTIONAL {
            debug!(
                message,
                intentional = INTENTIONAL,
                count = self.count,
                reason = self.reason,
                internal_log_rate_limit = true,
            );
        } else {
            error!(
                message,
                intentional = INTENTIONAL,
                count = self.count,
                reason = self.reason,
                internal_log_rate_limit = true,
            );
        }
        counter!(
            "component_discarded_events_total",
            self.count,
            "intentional" => if INTENTIONAL { "true" } else { "false" },
        );
    }
}

#[derive(Debug)]
pub struct SinkRequestBuildError<E> {
    pub name: &'static str,
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for SinkRequestBuildError<E> {
    fn emit(self) {
        error!(
            message = format!("Failed to build request for {}", self.name),
            error = %self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
    }
}

#[derive(Debug)]
pub struct SinkSendError<E> {
    pub message: &'static str,
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for SinkSendError<E> {
    fn emit(self) {
        error!(
            message = %self.message,
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
    }
}
