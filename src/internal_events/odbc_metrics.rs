use vector_common::internal_event::{CounterName, InternalEvent, error_stage, error_type};
use vector_lib::{NamedInternalEvent, counter};

#[derive(Debug, NamedInternalEvent)]
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
            CounterName::ComponentReceivedEventsTotal,
            "protocol" => "odbc"
        )
        .increment(self.count as u64);
        counter!(
            CounterName::ComponentReceivedEventBytesTotal,
            "protocol" => "odbc"
        )
        .increment(0);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct OdbcFailedError<'a> {
    pub statement: &'a str,
}

impl InternalEvent for OdbcFailedError<'_> {
    fn emit(self) {
        error!(
            message = "Unable to execute statement.",
            statement = %self.statement,
            error_type = error_type::COMMAND_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type::COMMAND_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
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
        counter!(CounterName::ComponentExecutedEventsTotal).increment(1);
    }
}
