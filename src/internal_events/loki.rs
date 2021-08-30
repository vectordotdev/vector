use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct LokiEventUnlabeled;

impl InternalEvent for LokiEventUnlabeled {
    fn emit_metrics(&self) {
        counter!("unlabeled_events_total", 1);
    }
}

#[derive(Debug)]
pub struct LokiEventsProcessed {
    pub byte_size: usize,
}

impl InternalEvent for LokiEventsProcessed {
    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct LokiUniqueStream;

impl InternalEvent for LokiUniqueStream {
    fn emit_metrics(&self) {
        counter!("streams_total", 1);
    }
}

#[derive(Debug)]
pub struct LokiOutOfOrderEventDropped;

impl InternalEvent for LokiOutOfOrderEventDropped {
    fn emit_logs(&self) {
        warn!(
            message = "Received out-of-order event; dropping event.",
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("events_discarded_total", 1);
        counter!("out_of_order_events_total", 1);
    }
}

#[derive(Debug)]
pub struct LokiOutOfOrderEventRewritten;

impl InternalEvent for LokiOutOfOrderEventRewritten {
    fn emit_logs(&self) {
        warn!(
            message = "Received out-of-order event, rewriting timestamp.",
            internal_log_rate_secs = 30
        );
    }

    fn emit_metrics(&self) {
        counter!("out_of_order_events_total", 1);
    }
}
