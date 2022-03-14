use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct RedisEventsSent {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for RedisEventsSent {
    fn emit_logs(&self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("component_sent_events_total", self.count as u64);
        counter!("component_sent_event_bytes_total", self.byte_size as u64);
        // deprecated
        counter!("processed_events_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct RedisSendEventError<'a> {
    pub error: &'a redis::RedisError,
}

impl<'a> InternalEvent for RedisSendEventError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to send message.",
            error = %self.error,
            error_code = %self.error.code().unwrap_or_default(),
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error.code().unwrap_or_default().to_string(),
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
        // deprecated
        counter!("send_errors_total", 1);
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
            error_code = %self.error.code().unwrap_or_default(),
            error_type = error_type::READER_FAILED,
            stage = error_stage::SENDING,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error.code().unwrap_or_default().to_string(),
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}
