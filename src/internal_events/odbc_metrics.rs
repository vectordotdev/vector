use vector_common::internal_event::{CounterName, InternalEvent};
use vector_lib::{NamedInternalEvent, counter};

use crate::sources::odbc::OdbcError;

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
    pub error: OdbcError,
}

impl InternalEvent for OdbcFailedError<'_> {
    fn emit(self) {
        let error_type = self.error.error_type();
        let stage = self.error.error_stage();

        error!(
            message = "Unable to execute statement.",
            statement = %self.statement,
            error = %self.error,
            error_type = error_type,
            stage = stage,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_type" => error_type,
            "stage" => stage,
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
