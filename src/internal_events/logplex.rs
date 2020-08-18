use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct HerokuLogplexRequestReceived<'a> {
    pub msg_count: usize,
    pub frame_id: &'a str,
    pub drain_token: &'a str,
}

impl<'a> InternalEvent for HerokuLogplexRequestReceived<'a> {
    fn emit_logs(&self) {
        info!(
            message = "Handling logplex request.",
            msg_count = %self.msg_count,
            frame_id = %self.frame_id,
            drain_token = %self.drain_token,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!("requests_received", 1,
            "component_kind" => "source",
            "component_type" => "logplex",
        );
    }
}

#[derive(Debug)]
pub struct HerokuLogplexRequestReadError {
    pub error: std::io::Error,
}

impl InternalEvent for HerokuLogplexRequestReadError {
    fn emit_logs(&self) {
        error!(
            message = "error reading request body.",
            error = ?self.error,
            rate_limit_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "request_read_errors", 1,
            "component_kind" => "source",
            "component_type" => "logplex",
        );
    }
}
