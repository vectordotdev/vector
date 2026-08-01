use vector_common::internal_event::{CounterName, InternalEvent, error_stage, error_type};
use vector_lib::source_sender::SendError;
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
            // `StreamClosedError` already incremented ComponentErrorsTotal and recorded
            // ComponentEventsDropped for the failed chunk. Keep the ODBC-context log only.
            OdbcError::SendError {
                source: SendError::Closed,
            }
            | OdbcError::SendFailedAfterCheckpoint {
                source: SendError::Closed,
                ..
            } => {
                error!(
                    message = "Unable to execute statement.",
                    statement = %self.statement,
                    error = %self.error,
                    error_type = error_type::WRITER_FAILED,
                    stage = error_stage::SENDING,
                );
            }
            OdbcError::SendError { .. } | OdbcError::SendFailedAfterCheckpoint { .. } => {
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
            OdbcError::Json { .. } | OdbcError::InvalidResultRow => {
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
            OdbcError::MissingTrackingColumn { .. }
            | OdbcError::InvalidTrackingValue { .. }
            | OdbcError::InvalidTrackingRow => {
                error!(
                    message = "Invalid ODBC tracking state.",
                    statement = %self.statement,
                    error = %self.error,
                    error_type = error_type::CONFIGURATION_FAILED,
                    stage = error_stage::PROCESSING,
                );
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_type" => error_type::CONFIGURATION_FAILED,
                    "stage" => error_stage::PROCESSING,
                )
                .increment(1);
            }
            OdbcError::DuplicateColumnNames { .. } => {
                error!(
                    message = "Query returned duplicate column names.",
                    statement = %self.statement,
                    error = %self.error,
                    error_type = error_type::CONFIGURATION_FAILED,
                    stage = error_stage::PROCESSING,
                );
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_type" => error_type::CONFIGURATION_FAILED,
                    "stage" => error_stage::PROCESSING,
                )
                .increment(1);
            }
            OdbcError::Shutdown | OdbcError::ShutdownAfterCheckpoint { .. } => {
                // Handled by the scheduler as a clean exit, not as a failure metric.
                // ComponentEventsDropped for ShutdownAfterCheckpoint is emitted by the client.
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
        counter!(CounterName::CollectCompletedTotal).increment(1);
    }
}
