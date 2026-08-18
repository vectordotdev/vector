use std::time::Duration;

use vector_lib::{
    NamedInternalEvent, counter, histogram,
    internal_event::{CounterName, HistogramName, InternalEvent, error_stage, error_type},
};

use crate::sources::azure_blob::queue::ProcessingError;

/// Render an error together with its full `source` chain.
///
/// `azure_core::Error` only renders its own context through `Display`, so the underlying
/// transport failure never reaches the log. Snafu variants already interpolate their source, so
/// a segment the accumulated text contains is skipped rather than repeated.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut rendered = error.to_string();
    let mut next = error.source();
    while let Some(error) = next {
        let segment = error.to_string();
        if !rendered.contains(&segment) {
            rendered.push_str(": ");
            rendered.push_str(&segment);
        }
        next = error.source();
    }
    rendered
}

#[derive(Debug, NamedInternalEvent)]
pub struct AzureBlobProcessingSucceeded<'a> {
    pub container: &'a str,
    pub duration: Duration,
}

impl InternalEvent for AzureBlobProcessingSucceeded<'_> {
    fn emit(self) {
        debug!(
            message = "Azure blob processing succeeded.",
            container = %self.container,
            duration_ms = %self.duration.as_millis(),
        );
        histogram!(
            HistogramName::AzureBlobProcessingSucceededDurationSeconds,
            "container" => self.container.to_owned(),
        )
        .record(self.duration);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AzureBlobProcessingFailed<'a> {
    pub container: &'a str,
    pub duration: Duration,
}

impl InternalEvent for AzureBlobProcessingFailed<'_> {
    fn emit(self) {
        debug!(
            message = "Azure blob processing failed.",
            container = %self.container,
            duration_ms = %self.duration.as_millis(),
        );
        histogram!(
            HistogramName::AzureBlobProcessingFailedDurationSeconds,
            "container" => self.container.to_owned(),
        )
        .record(self.duration);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AzureQueueMessageReceiveSucceeded {
    pub count: usize,
}

impl InternalEvent for AzureQueueMessageReceiveSucceeded {
    fn emit(self) {
        trace!(message = "Received Azure queue messages.", count = %self.count);
        counter!(CounterName::AzureQueueMessageReceiveSucceededTotal).increment(1);
        counter!(CounterName::AzureQueueMessageReceivedMessagesTotal).increment(self.count as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AzureQueueMessageReceiveError<'a> {
    pub error: &'a azure_core::Error,
}

impl InternalEvent for AzureQueueMessageReceiveError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to fetch Azure queue messages.",
            error = %error_chain(self.error),
            error_code = "failed_fetching_azure_queue_messages",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "failed_fetching_azure_queue_messages",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AzureQueueMessageProcessingSucceeded<'a> {
    pub message_id: &'a str,
}

impl InternalEvent for AzureQueueMessageProcessingSucceeded<'_> {
    fn emit(self) {
        trace!(message = "Processed Azure queue message successfully.", message_id = %self.message_id);
        counter!(CounterName::AzureQueueMessageProcessingSucceededTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AzureQueueMessageProcessingError<'a> {
    pub message_id: &'a str,
    pub error: &'a ProcessingError,
    pub dequeue_count: Option<i64>,
}

const PROCESSING_ERROR_CODE: &str = "failed_processing_azure_queue_message";

impl InternalEvent for AzureQueueMessageProcessingError<'_> {
    fn emit(self) {
        error!(
            message = "Failed to process Azure queue message.",
            message_id = %self.message_id,
            error = %error_chain(self.error),
            dequeue_count = self.dequeue_count,
            error_code = PROCESSING_ERROR_CODE,
            error_type = self.error.error_type(),
            stage = error_stage::PROCESSING,
        );

        // Spelled out per error kind rather than driven from `ProcessingError::error_type`
        // because `cargo vdev check events` requires a literal `error_type::` constant here.
        match self.error {
            ProcessingError::InvalidQueueMessage { .. }
            | ProcessingError::InvalidBlobPath { .. } => {
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_code" => PROCESSING_ERROR_CODE,
                    "error_type" => error_type::PARSER_FAILED,
                    "stage" => error_stage::PROCESSING,
                )
                .increment(1);
            }
            ProcessingError::ContainerClient { .. }
            | ProcessingError::ForeignStorageAccount { .. } => {
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_code" => PROCESSING_ERROR_CODE,
                    "error_type" => error_type::CONFIGURATION_FAILED,
                    "stage" => error_stage::PROCESSING,
                )
                .increment(1);
            }
            ProcessingError::GetBlob { .. } => {
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_code" => PROCESSING_ERROR_CODE,
                    "error_type" => error_type::REQUEST_FAILED,
                    "stage" => error_stage::PROCESSING,
                )
                .increment(1);
            }
            ProcessingError::ReadBlob { .. } => {
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_code" => PROCESSING_ERROR_CODE,
                    "error_type" => error_type::READER_FAILED,
                    "stage" => error_stage::PROCESSING,
                )
                .increment(1);
            }
            ProcessingError::PipelineSend { .. } => {
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_code" => PROCESSING_ERROR_CODE,
                    "error_type" => error_type::WRITER_FAILED,
                    "stage" => error_stage::PROCESSING,
                )
                .increment(1);
            }
            ProcessingError::ErrorAcknowledgement { .. }
            | ProcessingError::RejectedBySink { .. } => {
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_code" => PROCESSING_ERROR_CODE,
                    "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                    "stage" => error_stage::PROCESSING,
                )
                .increment(1);
            }
        }
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AzureQueueMessageDeleteSucceeded<'a> {
    pub message_id: &'a str,
}

impl InternalEvent for AzureQueueMessageDeleteSucceeded<'_> {
    fn emit(self) {
        trace!(message = "Deleted Azure queue message.", message_id = %self.message_id);
        counter!(CounterName::AzureQueueMessageDeleteSucceededTotal).increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AzureQueueMessageDeleteError<'a> {
    pub message_id: &'a str,
    pub error: &'a azure_core::Error,
}

impl InternalEvent for AzureQueueMessageDeleteError<'_> {
    fn emit(self) {
        error!(
            message = "Deletion of Azure queue message failed.",
            message_id = %self.message_id,
            error = %error_chain(self.error),
            error_code = "failed_deleting_azure_queue_message",
            error_type = error_type::ACKNOWLEDGMENT_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "failed_deleting_azure_queue_message",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct AzureBlobEventIgnored<'a> {
    pub event_type: &'a str,
}

impl InternalEvent for AzureBlobEventIgnored<'_> {
    fn emit(self) {
        debug!(
            message = "Ignored queue message for an event that was not BlobCreated.",
            event_type = %self.event_type,
        );
        counter!(
            CounterName::AzureBlobEventIgnoredTotal,
            "event_type" => self.event_type.to_owned(),
        )
        .increment(1);
    }
}

#[cfg(test)]
mod tests {
    use azure_core::error::ErrorKind;

    use super::error_chain;

    #[test]
    fn error_chain_appends_the_hidden_cause() {
        let cause = std::io::Error::other("failed to look up address information");
        let error = azure_core::Error::with_error(
            ErrorKind::Io,
            cause,
            "failed to execute `reqwest` request",
        );
        assert_eq!(error.to_string(), "failed to execute `reqwest` request");
        assert_eq!(
            error_chain(&error),
            "failed to execute `reqwest` request: failed to look up address information"
        );
    }

    #[test]
    fn error_chain_does_not_repeat_an_already_interpolated_source() {
        let cause = std::io::Error::other("connection refused");
        let error =
            azure_core::Error::with_error(ErrorKind::Io, cause, "outer: connection refused");

        assert_eq!(error_chain(&error), "outer: connection refused");
    }
}
