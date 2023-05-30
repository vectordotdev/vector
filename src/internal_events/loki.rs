use crate::emit;
use metrics::counter;
use vector_common::internal_event::error_stage;
use vector_core::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

#[derive(Debug)]
pub struct LokiEventUnlabeledError;

impl InternalEvent for LokiEventUnlabeledError {
    fn emit(self) {
        counter!(
            "component_errors_total", 1,
            "error_type" => "unlabeled_event",
            "stage" => error_stage::PROCESSING,
        );
    }
}

#[derive(Debug)]
pub struct LokiOutOfOrderEventDroppedError {
    pub count: usize,
}

impl InternalEvent for LokiOutOfOrderEventDroppedError {
    fn emit(self) {
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: self.count,
            reason: "out_of_order",
        });

        counter!(
            "component_errors_total", 1,
            "error_type" => "out_of_order",
            "stage" => error_stage::PROCESSING,
        );
    }
}

#[derive(Debug)]
pub struct LokiOutOfOrderEventRewritten {
    pub count: usize,
}

impl InternalEvent for LokiOutOfOrderEventRewritten {
    fn emit(self) {
        debug!(
            message = "Timestamps rewritten.",
            count = self.count,
            reason = "out_of_order",
            internal_log_rate_limit = true,
        );
        counter!("rewritten_timestamp_events_total", self.count as u64);
    }
}
