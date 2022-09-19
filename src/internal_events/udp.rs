use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::{
    emit,
    internal_events::{ComponentEventsDropped, UNINTENTIONAL},
};
use vector_common::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct UdpSocketConnectionEstablished;

impl InternalEvent for UdpSocketConnectionEstablished {
    fn emit(self) {
        debug!(message = "Connected.");
        counter!("connection_established_total", 1, "mode" => "udp");
    }
}

#[derive(Debug)]
pub struct UdpSocketSendError {
    pub error: std::io::Error,
}

impl InternalEvent for UdpSocketSendError {
    fn emit(self) {
        let reason = "UDP socket send error.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
        // deprecated
        counter!("connection_errors_total", 1, "mode" => "udp");

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}

#[derive(Debug)]
pub struct UdpSendIncompleteError {
    pub data_size: usize,
    pub sent: usize,
}

impl InternalEvent for UdpSendIncompleteError {
    fn emit(self) {
        let reason = "Could not send all data in one UDP packet.";
        error!(
            message = reason,
            data_size = self.data_size,
            sent = self.sent,
            dropped = self.data_size - self.sent,
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
        // deprecated
        counter!("connection_send_errors_total", 1, "mode" => "udp");

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}
