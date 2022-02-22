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
pub struct RedisSendEventError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for RedisSendEventError<E> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to send message.",
            error = %self.error,
            error_code = "redis_sending",
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::SENDING,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "redis_sending",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::SENDING,
        );
        // deprecated
        counter!("send_errors_total", 1);
    }
}
