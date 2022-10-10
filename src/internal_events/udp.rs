use metrics::counter;
use vector_common::internal_event::{error_stage, error_type};
use vector_core::internal_event::InternalEvent;

use crate::{emit, internal_events::SocketOutgoingConnectionError};

#[derive(Debug)]
pub struct UdpSocketConnectionEstablished;

impl InternalEvent for UdpSocketConnectionEstablished {
    fn emit(self) {
        debug!(message = "Connected.");
        counter!("connection_established_total", 1, "mode" => "udp");
    }
}

#[derive(Debug)]
pub struct UdpSocketOutgoingConnectionError<E> {
    pub error: E,
}

impl<E: std::error::Error> InternalEvent for UdpSocketOutgoingConnectionError<E> {
    fn emit(self) {
        // ## skip check-duplicate-events ##
        // ## skip check-validity-events ##
        emit!(SocketOutgoingConnectionError { error: self.error });
        // deprecated
        counter!("connection_failed_total", 1, "mode" => "udp");
    }
}

#[derive(Debug)]
pub struct UdpSocketError {
    pub error: std::io::Error,
}

impl InternalEvent for UdpSocketError {
    fn emit(self) {
        error!(
            message = "UDP socket error.",
            error = %self.error,
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("connection_errors_total", 1, "mode" => "udp");
    }
}

#[derive(Debug)]
pub struct UdpSendIncompleteError {
    pub data_size: usize,
    pub sent: usize,
}

impl InternalEvent for UdpSendIncompleteError {
    fn emit(self) {
        error!(
            message = "Could not send all data in one UDP packet; dropping some data.",
            data_size = self.data_size,
            sent = self.sent,
            dropped = self.data_size - self.sent,
            internal_log_rate_limit = true,
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("connection_send_errors_total", 1, "mode" => "udp");
    }
}
