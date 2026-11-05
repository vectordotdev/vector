use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{
        ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage, error_type,
    },
};

/// A request that failed with a transient error and exhausted its retries.
///
/// The events are finalized as `Errored` so acknowledging sources can redeliver them.
#[derive(NamedInternalEvent)]
pub(crate) struct GcpBigqueryRetriesExhaustedError {
    pub reason: String,
    pub count: usize,
}

impl InternalEvent for GcpBigqueryRetriesExhaustedError {
    fn emit(self) {
        let reason = "BigQuery request failed after exhausting retries.";
        error!(
            message = reason,
            error = %self.reason,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        )
        .increment(1);
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: self.count,
            reason,
        });
    }
}
