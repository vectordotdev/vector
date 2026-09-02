use vector_lib::internal_event::{
    ComponentEventsDropped, CounterName, InternalEvent, UNINTENTIONAL, error_stage, error_type,
};
use vector_lib::{NamedInternalEvent, counter};

#[derive(Debug, NamedInternalEvent)]
pub struct DatadogMetricsEncodingError<'a> {
    pub reason: &'a str,
    pub error_code: &'static str,
    pub dropped_events: usize,
}

impl InternalEvent for DatadogMetricsEncodingError<'_> {
    fn emit(self) {
        error!(
            message = self.reason,
            error_code = self.error_code,
            error_type = error_type::ENCODER_FAILED,
            intentional = "false",
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => self.error_code,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);

        if self.dropped_events > 0 {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.dropped_events,
                reason: self.reason,
            });
        }
    }
}

/// Fired on every failed attempt to send a Datadog metrics request (including ones that
/// will be retried), tagged with the target `uri` so a specific failure can be correlated
/// with the endpoint it was sent to.
///
/// This is diagnostic logging only — it does not increment `component_errors_total`, since
/// the generic request driver already counts the final, post-retry failure via `CallError`.
/// Deliberately not named `...Error`: `cargo vdev check events` treats any event ending in
/// `Error` as a terminal component error that MUST log at `error!` and increment
/// `component_errors_total`. This event fires once per retry attempt, so doing that would
/// inflate `component_errors_total` by the retry count instead of by 1 per failed flush.
#[derive(Debug, NamedInternalEvent)]
pub struct DatadogMetricsRequestFailed<'a> {
    pub error: &'a str,
    pub uri: &'a http::Uri,
}

impl InternalEvent for DatadogMetricsRequestFailed<'_> {
    fn emit(self) {
        warn!(
            message = "Failed to send Datadog metrics request.",
            error = self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
            uri = %self.uri,
            internal_log_rate_limit = true,
        );
    }
}
