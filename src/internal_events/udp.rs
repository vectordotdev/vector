use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct UdpSocketConnectionEstablished;

impl InternalEvent for UdpSocketConnectionEstablished {
    fn emit(self) {
        debug!(message = "Connected.");
        counter!("connection_established_total", 1, "mode" => "udp");
    }
}

#[derive(Debug)]
pub struct UdpSocketConnectionError<E> {
    pub error: E,
}

impl<E: std::error::Error> InternalEvent for UdpSocketConnectionError<E> {
    fn emit(self) {
        error!(
            message = "Unable to connect.",
            error = %self.error,
            error_code = "connection",
            error_type = error_type::READER_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "connection",
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
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
            internal_log_rate_secs = 30,
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
