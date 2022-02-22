use super::prelude::{error_stage, error_type};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct LokiEventUnlabeledError;

impl InternalEvent for LokiEventUnlabeledError {
    fn emit_logs(&self) {
        error!(
            message = "Unlabeled event, setting defaults.",
            error_code = "unlabeled",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "unlabeled",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "unlabeled_event",
        );
    }
}

#[derive(Debug)]
pub struct LokiEventsProcessed {
    pub byte_size: usize,
}

impl InternalEvent for LokiEventsProcessed {
    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64); // deprecated
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
pub struct LokiOutOfOrderEventDroppedError;

impl InternalEvent for LokiOutOfOrderEventDroppedError {
    fn emit_logs(&self) {
        error!(
            message = "Received out-of-order event; dropping event.",
            error_code = "out_of_order",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "out_of_order",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        counter!(
            "component_discarded_events_total", 1,
            "error_code" => "out_of_order",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("events_discarded_total", 1, "reason" => "out_of_order");
        counter!("processing_errors_total", 1, "error_type" => "out_of_order");
    }
}

#[derive(Debug)]
pub struct LokiOutOfOrderEventRewrittenError;

impl InternalEvent for LokiOutOfOrderEventRewrittenError {
    fn emit_logs(&self) {
        error!(
            message = "Received out-of-order event, rewriting timestamp.",
            error_code = "out_of_order",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_secs = 10
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "out_of_order",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!(
            "processing_errors_total", 1,
            "error_type" => "out_of_order"
        );
        counter!("rewritten_timestamp_events_total", 1);
    }
}
