use super::InternalEvent;
use metrics::counter;

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
            rate_limit_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("udp_send_errors", 1);
    }
}
