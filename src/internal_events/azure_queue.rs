#[cfg(feature = "sources-azure_blob")]
pub use azure_blob::*;
use metrics::counter;
use vector_lib::internal_event::{error_stage, error_type, InternalEvent};

#[cfg(feature = "sources-azure_blob")]
mod azure_blob {
    use super::*;
    use crate::event::Event;
    use crate::sources::azure_blob::queue::ProcessingError;

    #[derive(Debug)]
    pub struct QueueMessageProcessingError<'a> {
        pub message_id: &'a str,
        pub error: &'a ProcessingError,
    }

    impl<'a> InternalEvent for QueueMessageProcessingError<'a> {
        fn emit(self) {
            error!(
                message = "Failed to process Queue message.",
                message_id = %self.message_id,
                error = %self.error,
                error_code = "failed_processing_azure_queue_message",
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "failed_processing_azure_queue_message",
                "error_type" => error_type::PARSER_FAILED,
                "stage" => error_stage::PROCESSING,
            );
        }
    }

    #[derive(Debug)]
    pub struct InvalidRowEventType<'a> {
        pub event: &'a Event,
    }

    impl<'a> InternalEvent for InvalidRowEventType<'a> {
        fn emit(self) {
            error!(
                message = "Expected Azure rows as Log Events",
                event = ?self.event,
                error_code = "invalid_azure_row_event",
                error_type = error_type::CONDITION_FAILED,
                stage = error_stage::PROCESSING,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "invalid_azure_row_event",
                "error_type" => error_type::CONDITION_FAILED,
                "stage" => error_stage::PROCESSING,
            );
        }
    }
}

#[derive(Debug)]
pub struct QueueMessageReceiveError<'a, E> {
    pub error: &'a E,
}

impl<'a, E: std::fmt::Display> InternalEvent for QueueMessageReceiveError<'a, E> {
    fn emit(self) {
        error!(
            message = "Failed reading messages",
            error = %self.error,
            error_code = "failed_fetching_azure_queue_events",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_fetching_azure_queue_events",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}

#[derive(Debug)]
pub struct QueueMessageDeleteError<'a, E> {
    pub error: &'a E,
}

impl<'a, E: std::fmt::Display> InternalEvent for QueueMessageDeleteError<'a, E> {
    fn emit(self) {
        error!(
            message = "Failed deleting message",
            error = %self.error,
            error_code = "failed_deleting_azure_queue_event",
            error_type = error_type::ACKNOWLEDGMENT_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_deleting_azure_queue_event",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}

#[derive(Debug)]
pub struct QueueStorageInvalidEventIgnored<'a> {
    pub container: &'a str,
    pub subject: &'a str,
    pub event_type: &'a str,
}

impl<'a> InternalEvent for QueueStorageInvalidEventIgnored<'a> {
    fn emit(self) {
        trace!(
            message = "Ignoring event because of wrong event type",
            container = %self.container,
            subject = %self.subject,
            event_type = %self.event_type
        );
        counter!(
            "azure_queue_event_ignored_total", 1,
            "ignore_type" => "invalid_event_type"
        )
    }
}

#[derive(Debug)]
pub struct QueueMessageProcessingSucceeded {}

impl InternalEvent for QueueMessageProcessingSucceeded {
    fn emit(self) {
        trace!(message = "Processed azure queue message successfully.");
        counter!("azure_queue_message_processing_succeeded_total", 1);
    }
}

#[derive(Debug)]
pub struct QueueMessageProcessingErrored {}

impl InternalEvent for QueueMessageProcessingErrored {
    fn emit(self) {
        error!(message = "Batch event had a transient error in delivery.");
        counter!("azure_queue_message_processing_errored_total", 1);
    }
}

#[derive(Debug)]
pub struct QueueMessageProcessingRejected {}

impl InternalEvent for QueueMessageProcessingRejected {
    fn emit(self) {
        error!(message = "Batch event had a permanent failure or rejection.");
        counter!("azure_queue_message_processing_rejected_total", 1);
    }
}
