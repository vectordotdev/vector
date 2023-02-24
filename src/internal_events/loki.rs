use crate::emit;
use metrics::counter;
use vector_core::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

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
    pub count: usize,
}

impl InternalEvent for LokiOutOfOrderEventDropped {
    fn emit(self) {
        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: self.count,
            reason: "out_of_order",
        });

        // Deprecated
        counter!("events_discarded_total", self.count as u64,
                "reason" => "out_of_order");
        counter!("processing_errors_total", 1,
                "error_type" => "out_of_order");
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

        // Deprecated
        counter!("processing_errors_total", 1,
                "error_type" => "out_of_order");
    }
}
