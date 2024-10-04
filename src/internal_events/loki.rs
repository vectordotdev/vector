use metrics::counter;
use vector_lib::internal_event::{error_stage, error_type};
use vector_lib::internal_event::{ComponentEventsDropped, InternalEvent, INTENTIONAL};

#[derive(Debug)]
pub struct LokiEventUnlabeledError;

impl InternalEvent for LokiEventUnlabeledError {
    fn emit(self) {
        error!(
            message = "Event had no labels. Adding default `agent` label.",
            error_code = "unlabeled_event",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );

        counter!(
            "component_errors_total",
            "error_code" => "unlabeled_event",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct LokiOutOfOrderEventDroppedError {
    pub count: usize,
}

impl InternalEvent for LokiOutOfOrderEventDroppedError {
    fn emit(self) {
        let reason = "Dropping out-of-order event(s).";

        error!(
            message = reason,
            error_code = "out_of_order",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );

        emit!(ComponentEventsDropped::<INTENTIONAL> {
            count: self.count,
            reason,
        });

        counter!(
            "component_errors_total",
            "error_code" => "out_of_order",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
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
        counter!("rewritten_timestamp_events_total").increment(self.count as u64);
    }
}

#[derive(Debug)]
pub struct LokiTimestampNonParsableEventsDropped;

impl InternalEvent for LokiTimestampNonParsableEventsDropped {
    fn emit(self) {
        let reason = "Dropping timestamp non-parsable event(s).";

        error!(
            message = "Event timestamp non-parsable.",
            error_code = "non-parsable_timestamp",
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );

        emit!(ComponentEventsDropped::<INTENTIONAL> { count: 1, reason });

        counter!(
            "component_errors_total",
            "error_code" => "non-parsable_timestamp",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}
