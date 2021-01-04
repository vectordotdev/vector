use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct UdpSocketConnectionEstablished;

impl InternalEvent for UdpSocketConnectionEstablished {
    fn emit_logs(&self) {
        debug!(message = "Connected.");
    }

    fn emit_metrics(&self) {
        counter!("connection_established_total", 1, "mode" => "udp");
    }
}

#[derive(Debug)]
pub struct UdpSocketConnectionFailed<E> {
    pub error: E,
}

impl<E> InternalEvent for UdpSocketConnectionFailed<E>
where
    E: std::error::Error,
{
    fn emit_logs(&self) {
        error!(message = "Unable to connect.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("connection_failed_total", 1, "mode" => "udp");
    }
}

#[derive(Debug)]
pub struct UdpSocketError {
    pub error: std::io::Error,
}

impl InternalEvent for UdpSocketError {
    fn emit_logs(&self) {
        debug!(message = "UDP socket error.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("connection_errors_total", 1, "mode" => "udp");
    }
}

#[derive(Debug)]
pub struct UdpSendIncomplete {
    pub data_size: usize,
    pub sent: usize,
}

impl InternalEvent for UdpSendIncomplete {
    fn emit_logs(&self) {
        error!(
            message = "Could not send all data in one UDP packet; dropping some data.",
            data_size = self.data_size,
            sent = self.sent,
            dropped = self.data_size - self.sent,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("connection_send_errors_total", 1, "mode" => "udp");
    }
}
