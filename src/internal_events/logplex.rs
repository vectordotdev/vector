// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct HerokuLogplexRequestReceived<'a> {
    pub(crate) msg_count: usize,
    pub(crate) frame_id: &'a str,
    pub(crate) drain_token: &'a str,
}

impl<'a> InternalEvent for HerokuLogplexRequestReceived<'a> {
    fn emit_logs(&self) {
        info!(
            message = "Handling logplex request.",
            msg_count = %self.msg_count,
            frame_id = %self.frame_id,
            drain_token = %self.drain_token,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("requests_received_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct HerokuLogplexRequestReadError {
    pub(crate) error: std::io::Error,
}

impl InternalEvent for HerokuLogplexRequestReadError {
    fn emit_logs(&self) {
        error!(
            message = "Error reading request body.",
            error = ?self.error,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("request_read_errors_total", 1);
    }
}
