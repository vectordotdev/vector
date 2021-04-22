use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct RedisEventReceived {
    pub byte_size: usize,
}

impl InternalEvent for RedisEventReceived {
    fn emit_logs(&self) {
        trace!(message = "Received one event.", rate_limit_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct RedisReceiveEventFailed {
    pub error: redis::RedisError,
}

impl InternalEvent for RedisReceiveEventFailed {
    fn emit_logs(&self) {
        error!(
            message = "Failed to read message.",
            error = %self.error,
            rate_limit_secs = 30,
        );
    }
    fn emit_metrics(&self) {
        counter!("receive_event_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct RedisEventSent {
    pub byte_size: usize,
}

impl InternalEvent for RedisEventSent {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.", rate_limit_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct RedisSendEventFailed {
    pub error: redis::RedisError,
}

impl InternalEvent for RedisSendEventFailed {
    fn emit_logs(&self) {
        error!(
            message = "Failed to send message.",
            error = %self.error,
            rate_limit_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("send_event_errors_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct RedisEncodeEventFailed {
    pub error: serde_json::Error,
}

impl InternalEvent for RedisEncodeEventFailed {
    fn emit_logs(&self) {
        error!(
            message = "Error encoding Redis event to JSON.",
            error = ?self.error,
            rate_limit_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("encode_event_errors_total", 1);
    }
}
