use vector_common::internal_event::{CounterName, InternalEvent, error_stage, error_type};
use vector_lib::{NamedInternalEvent, counter};

use crate::sources::odbc::OdbcError;

#[derive(Debug, NamedInternalEvent)]
pub struct OdbcFailedError<'a> {
    pub statement: &'a str,
    pub error: OdbcError,
}

impl InternalEvent for OdbcFailedError<'_> {
    fn emit(self) {
        match self.error {
            OdbcError::Db { .. } | OdbcError::BlockingTask { .. } => {
                error!(
                    message = "Unable to execute statement.",
                    statement = %self.statement,
                    error = %self.error,
                    error_type = error_type::REQUEST_FAILED,
                    stage = error_stage::RECEIVING,
                );
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_type" => error_type::REQUEST_FAILED,
                    "stage" => error_stage::RECEIVING,
                )
                .increment(1);
            }
            OdbcError::Io { .. } => {
                error!(
                    message = "Unable to execute statement.",
                    statement = %self.statement,
                    error = %self.error,
                    error_type = error_type::IO_FAILED,
                    stage = error_stage::RECEIVING,
                );
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_type" => error_type::IO_FAILED,
                    "stage" => error_stage::RECEIVING,
                )
                .increment(1);
            }
            OdbcError::SendError { .. } => {
                error!(
                    message = "Unable to execute statement.",
                    statement = %self.statement,
                    error = %self.error,
                    error_type = error_type::WRITER_FAILED,
                    stage = error_stage::SENDING,
                );
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_type" => error_type::WRITER_FAILED,
                    "stage" => error_stage::SENDING,
                )
                .increment(1);
            }
            OdbcError::Json { .. } | OdbcError::Decode { .. } => {
                error!(
                    message = "Unable to execute statement.",
                    statement = %self.statement,
                    error = %self.error,
                    error_type = error_type::PARSER_FAILED,
                    stage = error_stage::PROCESSING,
                );
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_type" => error_type::PARSER_FAILED,
                    "stage" => error_stage::PROCESSING,
                )
                .increment(1);
            }
        }
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
