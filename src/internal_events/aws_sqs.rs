use metrics::counter;
#[cfg(feature = "sources-aws_s3")]
pub use s3::*;
use vector_lib::internal_event::InternalEvent;
#[cfg(any(feature = "sources-aws_s3", feature = "sources-aws_sqs"))]
use vector_lib::internal_event::{error_stage, error_type};

#[cfg(feature = "sources-aws_s3")]
mod s3 {
    use aws_sdk_sqs::types::{
        BatchResultErrorEntry, DeleteMessageBatchRequestEntry, DeleteMessageBatchResultEntry,
    };

    use super::*;
    use crate::sources::aws_s3::sqs::ProcessingError;

    #[derive(Debug)]
    pub struct SqsMessageProcessingError<'a> {
        pub message_id: &'a str,
        pub error: &'a ProcessingError,
    }

    impl<'a> InternalEvent for SqsMessageProcessingError<'a> {
        fn emit(self) {
            error!(
                message = "Failed to process SQS message.",
                message_id = %self.message_id,
                error = %self.error,
                error_code = "failed_processing_sqs_message",
                error_type = error_type::PARSER_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "failed_processing_sqs_message",
                "error_type" => error_type::PARSER_FAILED,
                "stage" => error_stage::PROCESSING,
            );
        }
    }

    #[derive(Debug)]
    pub struct SqsMessageDeleteSucceeded {
        pub message_ids: Vec<DeleteMessageBatchResultEntry>,
    }

    impl InternalEvent for SqsMessageDeleteSucceeded {
        fn emit(self) {
            trace!(message = "Deleted SQS message(s).",
            message_ids = %self.message_ids.iter()
                .map(|x| x.id.as_str())
                .collect::<Vec<_>>()
                .join(", "));
            counter!(
                "sqs_message_delete_succeeded_total",
                self.message_ids.len() as u64
            );
        }
    }

    #[derive(Debug)]
    pub struct SqsMessageDeletePartialError {
        pub entries: Vec<BatchResultErrorEntry>,
    }

    impl InternalEvent for SqsMessageDeletePartialError {
        fn emit(self) {
            error!(
                message = "Deletion of SQS message(s) failed.",
                message_ids = %self.entries.iter()
                    .map(|x| format!("{}/{}", x.id, x.code))
                    .collect::<Vec<_>>()
                    .join(", "),
                error_code = "failed_deleting_some_sqs_messages",
                error_type = error_type::ACKNOWLEDGMENT_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "failed_deleting_some_sqs_messages",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::PROCESSING,
            );
        }
    }

    #[derive(Debug)]
    pub struct SqsMessageDeleteBatchError<E> {
        pub entries: Vec<DeleteMessageBatchRequestEntry>,
        pub error: E,
    }

    impl<E: std::fmt::Display> InternalEvent for SqsMessageDeleteBatchError<E> {
        fn emit(self) {
            error!(
                message = "Deletion of SQS message(s) failed.",
                message_ids = %self.entries.iter()
                    .map(|x| x.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                error = %self.error,
                error_code = "failed_deleting_all_sqs_messages",
                error_type = error_type::ACKNOWLEDGMENT_FAILED,
                stage = error_stage::PROCESSING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "failed_deleting_all_sqs_messages",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::PROCESSING,
            );
        }
    }
}

#[derive(Debug)]
pub struct SqsMessageReceiveError<'a, E> {
    pub error: &'a E,
}

impl<'a, E: std::fmt::Display> InternalEvent for SqsMessageReceiveError<'a, E> {
    fn emit(self) {
        error!(
            message = "Failed to fetch SQS events.",
            error = %self.error,
            error_code = "failed_fetching_sqs_events",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_fetching_sqs_events",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
    }
}

#[derive(Debug)]
pub struct SqsMessageReceiveSucceeded {
    pub count: usize,
}

impl InternalEvent for SqsMessageReceiveSucceeded {
    fn emit(self) {
        trace!(message = "Received SQS messages.", count = %self.count);
        counter!("sqs_message_receive_succeeded_total", 1);
        counter!("sqs_message_received_messages_total", self.count as u64);
    }
}

#[derive(Debug)]
pub struct SqsMessageProcessingSucceeded<'a> {
    pub message_id: &'a str,
}

impl<'a> InternalEvent for SqsMessageProcessingSucceeded<'a> {
    fn emit(self) {
        trace!(message = "Processed SQS message successfully.", message_id = %self.message_id);
        counter!("sqs_message_processing_succeeded_total", 1);
    }
}

// AWS SQS source

#[cfg(feature = "sources-aws_sqs")]
#[derive(Debug)]
pub struct SqsMessageDeleteError<'a, E> {
    pub error: &'a E,
}

#[cfg(feature = "sources-aws_sqs")]
impl<'a, E: std::fmt::Display> InternalEvent for SqsMessageDeleteError<'a, E> {
    fn emit(self) {
        error!(
            message = "Failed to delete SQS events.",
            error = %self.error,
            error_type = error_type::WRITER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
    }
}

// AWS s3 source

#[derive(Debug)]
pub struct SqsS3EventRecordInvalidEventIgnored<'a> {
    pub bucket: &'a str,
    pub key: &'a str,
    pub kind: &'a str,
    pub name: &'a str,
}

impl<'a> InternalEvent for SqsS3EventRecordInvalidEventIgnored<'a> {
    fn emit(self) {
        warn!(message = "Ignored S3 record in SQS message for an event that was not ObjectCreated.",
            bucket = %self.bucket, key = %self.key, kind = %self.kind, name = %self.name);
        counter!("sqs_s3_event_record_ignored_total", 1, "ignore_type" => "invalid_event_kind");
    }
}
