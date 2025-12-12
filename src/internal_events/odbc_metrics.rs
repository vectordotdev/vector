use metrics::counter;
use vector_common::internal_event::{InternalEvent, error_type};

#[derive(Debug)]
pub struct OdbcEventsReceived {
    pub count: usize,
}

impl InternalEvent for OdbcEventsReceived {
    fn emit(self) {
        trace!(
            message = "Events received.",
            count = %self.count,
        );
        counter!(
            "component_received_events_total",
            "protocol" => "odbc"
        )
        .increment(self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            "protocol" => "odbc"
        )
        .increment(0);
    }
}

#[derive(Debug)]
pub struct OdbcFailedError<'a> {
    pub statement: &'a str,
}

impl InternalEvent for OdbcFailedError<'_> {
    fn emit(self) {
        error!(
            message = "Unable to execute statement.",
            statement = %self.statement,
            error = error_type::COMMAND_FAILED
        );
        counter!(
            "component_errors_total",
            "statement" => self.statement.to_owned(),
            "error_type" => error_type::COMMAND_FAILED
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct OdbcQueryExecuted<'a> {
    pub statement: &'a str,
    pub elapsed: u128,
}

impl InternalEvent for OdbcQueryExecuted<'_> {
    fn emit(self) {
        trace!(
            message = "Executed statement.",
            statement = %self.statement,
            elapsedMs = %self.elapsed
        );
        counter!("component_executed_events_total").increment(1);
    }
}
