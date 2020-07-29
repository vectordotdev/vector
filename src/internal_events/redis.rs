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
pub struct RedisEventReceivedFail {
    pub error: redis::RedisError,
}

impl InternalEvent for RedisEventReceivedFail {
    fn emit_logs(&self) {
        error!(
            message = "Failed to read message.",
            error = ?self.error ,
            rate_limit_secs = 30,
        );
    }
    fn emit_metrics(&self) {
        counter!("events_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct RedisEventSend {
    pub byte_size: usize,
}

impl InternalEvent for RedisEventSend {
    fn emit_logs(&self) {
        trace!(message = "Processed one event.", rate_limit_secs = 10);
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct RedisEventSendFail {
    pub error: redis::RedisError,
}

impl InternalEvent for RedisEventSendFail {
    fn emit_logs(&self) {
        error!(
            message = "Failed to send message.",
            error = %self.error,
            rate_limit_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("send_errors_total", 1);
    }
}

#[derive(Debug)]
pub(crate) struct RedisEventEncodeError {
    pub error: serde_json::Error,
}

impl InternalEvent for RedisEventEncodeError {
    fn emit_logs(&self) {
        error!(
            message = "Error encoding redis event to JSON.",
            error = ?self.error,
            rate_limit_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("encode_errors_total", 1);
    }
}

#[derive(Debug)]
pub struct RedisMissingKeys<'a> {
    pub keys: &'a [String],
}

impl<'a> InternalEvent for RedisMissingKeys<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Keys do not exist on the event; dropping event.",
            missing_keys = ?self.keys,
            rate_limit_secs = 30,
        )
    }

    fn emit_metrics(&self) {
        counter!("missing_keys_total", 1);
    }
}
