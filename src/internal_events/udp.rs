use metrics::counter;
use vector_lib::internal_event::{
    error_stage, error_type, ComponentEventsDropped, InternalEvent, UNINTENTIONAL,
};

use crate::internal_events::SocketOutgoingConnectionError;

// TODO: Get rid of this. UDP is connectionless, so there's no "successful" connect event, only
// successfully binding a socket that can be used for receiving.
#[derive(Debug)]
pub struct UdpSocketConnectionEstablished;

impl InternalEvent for UdpSocketConnectionEstablished {
    fn emit(self) {
        debug!(message = "Connected.");
        counter!("connection_established_total", 1, "mode" => "udp");
    }
}

// TODO: Get rid of this. UDP is connectionless, so there's no "unsuccessful" connect event, only
// unsuccessfully binding a socket that can be used for receiving.
pub struct UdpSocketOutgoingConnectionError<E> {
    pub error: E,
}

impl<E: std::error::Error> InternalEvent for UdpSocketOutgoingConnectionError<E> {
    fn emit(self) {
        // ## skip check-duplicate-events ##
        // ## skip check-validity-events ##
        emit!(SocketOutgoingConnectionError { error: self.error });
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
