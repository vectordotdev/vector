use crate::{
    emit,
    internal_events::{ComponentEventsDropped, INTENTIONAL},
};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct LokiEventUnlabeled;

impl InternalEvent for LokiEventUnlabeled {
    fn emit(self) {
        // Deprecated
        counter!("processing_errors_total", 1,
                "error_type" => "unlabeled_event");
    }
}

#[derive(Debug)]
pub struct LokiOutOfOrderEventDropped {
    pub count: u64,
}

impl InternalEvent for LokiOutOfOrderEventDropped {
    fn emit(self) {
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: self.count,
            reason: "out_of_order",
        });

        // Deprecated
        counter!("events_discarded_total", self.count,
                "reason" => "out_of_order");
        counter!("processing_errors_total", 1,
                "error_type" => "out_of_order");
    }
}

#[derive(Debug)]
pub struct LokiOutOfOrderEventRewritten {
    pub count: u64,
}

impl InternalEvent for LokiOutOfOrderEventRewritten {
    fn emit(self) {
        debug!(
            message = "Timestamps rewritten.",
            count = self.count,
            reason = "out_of_order",
            internal_log_rate_limit = true,
        );
        counter!("rewritten_timestamp_events_total", self.count);

        // Deprecated
        counter!("processing_errors_total", 1,
                "error_type" => "out_of_order");
    }
}
