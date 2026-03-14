#![allow(dead_code)] // TODO requires optional feature compilation

use metrics::counter;
#[cfg(feature = "sources-aws_s3")]
pub use s3::*;
#[cfg(any(feature = "sources-aws_s3", feature = "sources-aws_sqs"))]
use vector_lib::internal_event::{error_stage, error_type};
use vector_lib::{NamedInternalEvent, internal_event::InternalEvent};

#[cfg(feature = "sources-aws_s3")]
mod s3 {
    use std::time::Duration;

    use aws_sdk_sqs::types::{
        BatchResultErrorEntry, DeleteMessageBatchRequestEntry, DeleteMessageBatchResultEntry,
        SendMessageBatchRequestEntry, SendMessageBatchResultEntry,
    };
    use metrics::histogram;

    use aws_smithy_types::error::display::DisplayErrorContext;

    use super::*;
    use crate::aws::error::{AwsErrorClass, classify_error};
    use crate::sources::aws_s3::sqs::ProcessingError;

    /// Returns an actionable hint for S3 source errors based on classification.
    fn s3_error_hint(class: AwsErrorClass, error_code: Option<&str>) -> Option<&'static str> {
        match (class, error_code) {
            (AwsErrorClass::Auth, Some("ExpiredToken" | "ExpiredTokenException")) => Some(
                "The security token has expired. Check credential refresh configuration.",
            ),
            (AwsErrorClass::Auth, _) => Some(
                "Check that the IAM role/user has s3:GetObject permission on this bucket/key.",
            ),
            (AwsErrorClass::NotFound, Some("NoSuchBucket")) => {
                Some("The S3 bucket does not exist. Check the bucket name in your configuration.")
            }
            (AwsErrorClass::NotFound, _) => Some(
                "The S3 object does not exist. It may have been deleted before Vector could fetch it.",
            ),
            (AwsErrorClass::Throttling, _) => Some(
                "AWS is throttling requests. Consider reducing poll frequency or request rate.",
            ),
            (AwsErrorClass::Connectivity, _) => Some(
                "Network connectivity issue. Check DNS, proxy, and TLS configuration.",
            ),
            (AwsErrorClass::Configuration, _) => Some(
                "Configuration error. Check credentials, region, and endpoint settings.",
            ),
            _ => None,
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct S3ObjectProcessingSucceeded<'a> {
        pub bucket: &'a str,
        pub duration: Duration,
    }

    impl InternalEvent for S3ObjectProcessingSucceeded<'_> {
        fn emit(self) {
            debug!(
                message = "S3 object processing succeeded.",
                bucket = %self.bucket,
                duration_ms = %self.duration.as_millis(),
            );
            histogram!(
                "s3_object_processing_succeeded_duration_seconds",
                "bucket" => self.bucket.to_owned(),
            )
            .record(self.duration);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct S3ObjectProcessingFailed<'a> {
        pub bucket: &'a str,
        pub key: &'a str,
        pub error: &'a str,
        pub duration: Duration,
    }

    impl InternalEvent for S3ObjectProcessingFailed<'_> {
        fn emit(self) {
            warn!(
                message = "S3 object processing failed.",
                bucket = %self.bucket,
                key = %self.key,
                error = %self.error,
                duration_ms = %self.duration.as_millis(),
            );
            histogram!(
                "s3_object_processing_failed_duration_seconds",
                "bucket" => self.bucket.to_owned(),
            )
            .record(self.duration);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct S3ObjectGetFailed<'a> {
        pub bucket: &'a str,
        pub key: &'a str,
        pub error_kind: &'a str,
        pub actionable_message: &'a str,
    }

    impl InternalEvent for S3ObjectGetFailed<'_> {
        fn emit(self) {
            error!(
                message = %self.actionable_message,
                bucket = %self.bucket,
                key = %self.key,
                error_kind = %self.error_kind,
                error_code = "failed_getting_s3_object",
                error_type = error_type::REQUEST_FAILED,
                stage = error_stage::RECEIVING,
            );
            counter!(
                "component_errors_total",
                "error_code" => "failed_getting_s3_object",
                "error_type" => error_type::REQUEST_FAILED,
                "stage" => error_stage::RECEIVING,
            )
            .increment(1);
            counter!(
                "s3_object_get_failed_total",
                "bucket" => self.bucket.to_owned(),
                "error_kind" => self.error_kind.to_owned(),
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct SqsMessageProcessingError<'a> {
        pub message_id: &'a str,
        pub error: &'a ProcessingError,
    }

    impl SqsMessageProcessingError<'_> {
        /// Returns the `(error_type, error_code)` pair for this processing error.
        const fn classify(error: &ProcessingError) -> (&'static str, &'static str) {
            match error {
                ProcessingError::GetObject { .. } => {
                    (error_type::REQUEST_FAILED, "failed_s3_get_object")
                }
                ProcessingError::InvalidSqsMessage { .. } => {
                    (error_type::PARSER_FAILED, "invalid_sqs_message")
                }
                ProcessingError::ReadObject { .. } => {
                    (error_type::READER_FAILED, "failed_reading_s3_object")
                }
                ProcessingError::PipelineSend { .. } => {
                    (error_type::WRITER_FAILED, "failed_sending_to_pipeline")
                }
                ProcessingError::WrongRegion { .. } => {
                    (error_type::CONDITION_FAILED, "wrong_region")
                }
                ProcessingError::UnsupportedS3EventVersion { .. } => {
                    (error_type::PARSER_FAILED, "unsupported_s3_event_version")
                }
                ProcessingError::ErrorAcknowledgement { .. } => {
                    (error_type::ACKNOWLEDGMENT_FAILED, "error_acknowledgement")
                }
                ProcessingError::FileTooOld { .. } => {
                    (error_type::CONDITION_FAILED, "file_too_old")
                }
            }
        }
    }

    impl InternalEvent for SqsMessageProcessingError<'_> {
        fn emit(self) {
            let (error_type_val, error_code_val) = Self::classify(self.error);

            if let ProcessingError::GetObject { source, bucket, key } = self.error {
                let ctx = crate::aws::error::extract_error_context(source);
                let class = classify_error(&ctx);
                error!(
                    message = "Failed to process SQS message.",
                    message_id = %self.message_id,
                    error = %DisplayErrorContext(source),
                    error_code = error_code_val,
                    error_type = error_type_val,
                    stage = error_stage::PROCESSING,
                    bucket = %bucket,
                    key = %key,
                    aws_error_code = ctx.aws_error_code.unwrap_or(""),
                    aws_http_status = ctx.http_status.unwrap_or(0),
                    aws_request_id = ctx.aws_request_id.unwrap_or(""),
                    aws_error_class = ?class,
                );
                if let Some(hint) = s3_error_hint(class, ctx.aws_error_code) {
                    warn!(message = %hint);
                }
            } else {
                error!(
                    message = "Failed to process SQS message.",
                    message_id = %self.message_id,
                    error = %self.error,
                    error_code = error_code_val,
                    error_type = error_type_val,
                    stage = error_stage::PROCESSING,
                );
            }

            counter!(
                "component_errors_total",
                "error_code" => error_code_val,
                "error_type" => error_type_val,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
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
            counter!("sqs_message_delete_succeeded_total").increment(self.message_ids.len() as u64);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
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
            );
            counter!(
                "component_errors_total",
                "error_code" => "failed_deleting_some_sqs_messages",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct SqsMessageDeleteBatchError<'a, E> {
        pub entries: Vec<DeleteMessageBatchRequestEntry>,
        pub error: E,
        pub aws_ctx: Option<crate::aws::error::AwsErrorContext<'a>>,
    }

    impl<E: std::fmt::Display> InternalEvent for SqsMessageDeleteBatchError<'_, E> {
        fn emit(self) {
            let message_ids = self
                .entries
                .iter()
                .map(|x| x.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if let Some(ref ctx) = self.aws_ctx {
                let class = crate::aws::error::classify_error(ctx);
                error!(
                    message = "Deletion of SQS message(s) failed.",
                    message_ids = %message_ids,
                    error = %self.error,
                    error_code = "failed_deleting_all_sqs_messages",
                    error_type = error_type::ACKNOWLEDGMENT_FAILED,
                    stage = error_stage::PROCESSING,
                    aws_error_code = ctx.aws_error_code.unwrap_or(""),
                    aws_http_status = ctx.http_status.unwrap_or(0),
                    aws_request_id = ctx.aws_request_id.unwrap_or(""),
                    aws_error_class = ?class,
                );
            } else {
                error!(
                    message = "Deletion of SQS message(s) failed.",
                    message_ids = %message_ids,
                    error = %self.error,
                    error_code = "failed_deleting_all_sqs_messages",
                    error_type = error_type::ACKNOWLEDGMENT_FAILED,
                    stage = error_stage::PROCESSING,
                );
            }
            counter!(
                "component_errors_total",
                "error_code" => "failed_deleting_all_sqs_messages",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct SqsMessageSentSucceeded {
        pub message_ids: Vec<SendMessageBatchResultEntry>,
    }

    impl InternalEvent for SqsMessageSentSucceeded {
        fn emit(self) {
            trace!(message = "Deferred SQS message(s).",
            message_ids = %self.message_ids.iter()
                .map(|x| x.id.as_str())
                .collect::<Vec<_>>()
                .join(", "));
            counter!("sqs_message_defer_succeeded_total").increment(self.message_ids.len() as u64);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct SqsMessageSentPartialError {
        pub entries: Vec<BatchResultErrorEntry>,
    }

    impl InternalEvent for SqsMessageSentPartialError {
        fn emit(self) {
            error!(
                message = "Sending of deferred SQS message(s) failed.",
                message_ids = %self.entries.iter()
                    .map(|x| format!("{}/{}", x.id, x.code))
                    .collect::<Vec<_>>()
                    .join(", "),
                error_code = "failed_deferring_some_sqs_messages",
                error_type = error_type::ACKNOWLEDGMENT_FAILED,
                stage = error_stage::PROCESSING,
            );
            counter!(
                "component_errors_total",
                "error_code" => "failed_deferring_some_sqs_messages",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, NamedInternalEvent)]
    pub struct SqsMessageSendBatchError<'a, E> {
        pub entries: Vec<SendMessageBatchRequestEntry>,
        pub error: E,
        pub aws_ctx: Option<crate::aws::error::AwsErrorContext<'a>>,
    }

    impl<E: std::fmt::Display> InternalEvent for SqsMessageSendBatchError<'_, E> {
        fn emit(self) {
            let message_ids = self
                .entries
                .iter()
                .map(|x| x.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if let Some(ref ctx) = self.aws_ctx {
                let class = crate::aws::error::classify_error(ctx);
                error!(
                    message = "Sending of deferred SQS message(s) failed.",
                    message_ids = %message_ids,
                    error = %self.error,
                    error_code = "failed_deferring_all_sqs_messages",
                    error_type = error_type::ACKNOWLEDGMENT_FAILED,
                    stage = error_stage::PROCESSING,
                    aws_error_code = ctx.aws_error_code.unwrap_or(""),
                    aws_http_status = ctx.http_status.unwrap_or(0),
                    aws_request_id = ctx.aws_request_id.unwrap_or(""),
                    aws_error_class = ?class,
                );
            } else {
                error!(
                    message = "Sending of deferred SQS message(s) failed.",
                    message_ids = %message_ids,
                    error = %self.error,
                    error_code = "failed_deferring_all_sqs_messages",
                    error_type = error_type::ACKNOWLEDGMENT_FAILED,
                    stage = error_stage::PROCESSING,
                );
            }
            counter!(
                "component_errors_total",
                "error_code" => "failed_deferring_all_sqs_messages",
                "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
                "stage" => error_stage::PROCESSING,
            )
            .increment(1);
        }
    }
}

/// Returns an actionable hint for SQS operation errors based on classification.
#[cfg(any(feature = "sources-aws_s3", feature = "sources-aws_sqs"))]
fn sqs_receive_error_hint(
    class: crate::aws::error::AwsErrorClass,
    error_code: Option<&str>,
) -> Option<&'static str> {
    use crate::aws::error::AwsErrorClass;
    match (class, error_code) {
        (AwsErrorClass::Auth, Some("ExpiredToken" | "ExpiredTokenException")) => Some(
            "The security token has expired. Check credential refresh configuration.",
        ),
        (AwsErrorClass::Auth, _) => Some(
            "Check that the IAM role/user has sqs:ReceiveMessage permission on this queue.",
        ),
        (AwsErrorClass::NotFound, _) => Some(
            "The SQS queue does not exist. Check the queue URL, region, and account in your configuration.",
        ),
        (AwsErrorClass::Throttling, _) => Some(
            "AWS is throttling SQS requests. Consider reducing poll frequency or concurrency.",
        ),
        (AwsErrorClass::Connectivity, _) => {
            Some("Network connectivity issue. Check DNS, proxy, and TLS configuration.")
        }
        (AwsErrorClass::Configuration, _) => {
            Some("Configuration error. Check credentials, region, and endpoint settings.")
        }
        (AwsErrorClass::ServiceError, _) => {
            Some("AWS SQS returned a server error. This is usually transient; retries should resolve it.")
        }
        _ => None,
    }
}

/// Returns an actionable hint for SQS delete errors based on classification.
#[cfg(feature = "sources-aws_sqs")]
fn sqs_delete_error_hint(
    class: crate::aws::error::AwsErrorClass,
    error_code: Option<&str>,
) -> Option<&'static str> {
    use crate::aws::error::AwsErrorClass;
    match (class, error_code) {
        (AwsErrorClass::Auth, Some("ExpiredToken" | "ExpiredTokenException")) => Some(
            "The security token has expired. Check credential refresh configuration.",
        ),
        (AwsErrorClass::Auth, _) => Some(
            "Check that the IAM role/user has sqs:DeleteMessage permission on this queue.",
        ),
        (AwsErrorClass::NotFound, _) => Some(
            "The SQS queue does not exist. Check the queue URL, region, and account in your configuration.",
        ),
        (AwsErrorClass::Throttling, _) => Some(
            "AWS is throttling SQS requests. Consider reducing request rate.",
        ),
        _ => None,
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct SqsMessageReceiveError<'a, E> {
    pub error: &'a E,
    pub aws_ctx: Option<crate::aws::error::AwsErrorContext<'a>>,
}

impl<E: std::fmt::Display> InternalEvent for SqsMessageReceiveError<'_, E> {
    fn emit(self) {
        if let Some(ref ctx) = self.aws_ctx {
            let class = crate::aws::error::classify_error(ctx);
            error!(
                message = "Failed to fetch SQS events.",
                error = %self.error,
                error_code = "failed_fetching_sqs_events",
                error_type = error_type::REQUEST_FAILED,
                stage = error_stage::RECEIVING,
                aws_error_code = ctx.aws_error_code.unwrap_or(""),
                aws_http_status = ctx.http_status.unwrap_or(0),
                aws_request_id = ctx.aws_request_id.unwrap_or(""),
                aws_error_class = ?class,
            );
            if let Some(hint) = sqs_receive_error_hint(class, ctx.aws_error_code) {
                warn!(message = %hint);
            }
        } else {
            error!(
                message = "Failed to fetch SQS events.",
                error = %self.error,
                error_code = "failed_fetching_sqs_events",
                error_type = error_type::REQUEST_FAILED,
                stage = error_stage::RECEIVING,
            );
        }
        counter!(
            "component_errors_total",
            "error_code" => "failed_fetching_sqs_events",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct SqsMessageReceiveSucceeded {
    pub count: usize,
}

impl InternalEvent for SqsMessageReceiveSucceeded {
    fn emit(self) {
        trace!(message = "Received SQS messages.", count = %self.count);
        counter!("sqs_message_receive_succeeded_total").increment(1);
        counter!("sqs_message_received_messages_total").increment(self.count as u64);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct SqsMessageProcessingSucceeded<'a> {
    pub message_id: &'a str,
}

impl InternalEvent for SqsMessageProcessingSucceeded<'_> {
    fn emit(self) {
        trace!(message = "Processed SQS message successfully.", message_id = %self.message_id);
        counter!("sqs_message_processing_succeeded_total").increment(1);
    }
}

// AWS SQS source

#[cfg(feature = "sources-aws_sqs")]
#[derive(Debug, NamedInternalEvent)]
pub struct SqsMessageDeleteError<'a, E> {
    pub error: &'a E,
    pub aws_ctx: Option<crate::aws::error::AwsErrorContext<'a>>,
}

#[cfg(feature = "sources-aws_sqs")]
impl<E: std::fmt::Display> InternalEvent for SqsMessageDeleteError<'_, E> {
    fn emit(self) {
        if let Some(ref ctx) = self.aws_ctx {
            let class = crate::aws::error::classify_error(ctx);
            error!(
                message = "Failed to delete SQS events.",
                error = %self.error,
                error_type = error_type::WRITER_FAILED,
                stage = error_stage::PROCESSING,
                aws_error_code = ctx.aws_error_code.unwrap_or(""),
                aws_http_status = ctx.http_status.unwrap_or(0),
                aws_request_id = ctx.aws_request_id.unwrap_or(""),
                aws_error_class = ?class,
            );
            if let Some(hint) = sqs_delete_error_hint(class, ctx.aws_error_code) {
                warn!(message = %hint);
            }
        } else {
            error!(
                message = "Failed to delete SQS events.",
                error = %self.error,
                error_type = error_type::WRITER_FAILED,
                stage = error_stage::PROCESSING,
            );
        }
        counter!(
            "component_errors_total",
            "error_type" => error_type::WRITER_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

// AWS s3 source

#[derive(Debug, NamedInternalEvent)]
pub struct SqsS3EventRecordInvalidEventIgnored<'a> {
    pub bucket: &'a str,
    pub key: &'a str,
    pub kind: &'a str,
    pub name: &'a str,
}

impl InternalEvent for SqsS3EventRecordInvalidEventIgnored<'_> {
    fn emit(self) {
        warn!(message = "Ignored S3 record in SQS message for an event that was not ObjectCreated.",
            bucket = %self.bucket, key = %self.key, kind = %self.kind, name = %self.name);
        counter!("sqs_s3_event_record_ignored_total", "ignore_type" => "invalid_event_kind")
            .increment(1);
    }
}
