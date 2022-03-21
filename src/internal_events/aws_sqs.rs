use metrics::counter;
#[cfg(feature = "sources-aws_s3")]
use rusoto_core::RusotoError;
#[cfg(feature = "sources-aws_s3")]
use rusoto_sqs::{
    BatchResultErrorEntry, DeleteMessageBatchError, DeleteMessageBatchRequestEntry,
    DeleteMessageBatchResultEntry, ReceiveMessageError,
};
use vector_core::internal_event::InternalEvent;

#[cfg(feature = "sources-aws_s3")]
use crate::internal_events::prelude::{error_stage, error_type};

#[cfg(feature = "sources-aws_s3")]
use crate::sources::aws_s3::sqs::ProcessingError;

#[derive(Debug)]
pub struct AwsSqsEventsSent<'a> {
    pub byte_size: usize,
    pub message_id: Option<&'a String>,
}

impl InternalEvent for AwsSqsEventsSent<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Events sent.",
            message_id = ?self.message_id,
            count = 1,
            byte_size = %self.byte_size,
        );
    }

    fn emit_metrics(&self) {
        counter!("component_sent_events_total", 1);
        counter!("component_sent_event_bytes_total", self.byte_size as u64);
        // deprecated
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct SqsS3EventsReceived {
    pub byte_size: usize,
}

impl InternalEvent for SqsS3EventsReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Events received.",
            count = 1,
            byte_size = %self.byte_size,
        );
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", 1);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        // deprecated
        counter!("events_in_total", 1);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[cfg(feature = "sources-aws_s3")]
#[derive(Debug)]
pub struct SqsMessageReceiveError<'a> {
    pub error: &'a RusotoError<ReceiveMessageError>,
}

#[cfg(feature = "sources-aws_s3")]
impl<'a> InternalEvent for SqsMessageReceiveError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to fetch SQS events.",
            error = %self.error,
            error_code = "failed_fetching_sqs_events",
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::RECEIVING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_fetching_sqs_events",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("sqs_message_receive_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct SqsMessageReceiveSucceeded {
    pub count: usize,
}

impl InternalEvent for SqsMessageReceiveSucceeded {
    fn emit_logs(&self) {
        trace!(message = "Received SQS messages.", count = %self.count);
    }

    fn emit_metrics(&self) {
        counter!("sqs_message_receive_succeeded_total", 1);
        counter!("sqs_message_received_messages_total", self.count as u64);
    }
}

#[derive(Debug)]
pub struct SqsMessageProcessingSucceeded<'a> {
    pub message_id: &'a str,
}

impl<'a> InternalEvent for SqsMessageProcessingSucceeded<'a> {
    fn emit_logs(&self) {
        trace!(message = "Processed SQS message succeededly.", message_id = %self.message_id);
    }

    fn emit_metrics(&self) {
        counter!("sqs_message_processing_succeeded_total", 1);
    }
}

#[cfg(feature = "sources-aws_s3")]
#[derive(Debug)]
pub struct SqsMessageProcessingError<'a> {
    pub message_id: &'a str,
    pub error: &'a ProcessingError,
}

#[cfg(feature = "sources-aws_s3")]
impl<'a> InternalEvent for SqsMessageProcessingError<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Failed to process SQS message.",
            message_id = %self.message_id,
            error = %self.error,
            error_code = "failed_processing_sqs_message",
            error_type = error_type::PARSER_FAILED,
            stage = error_stage::PROCESSING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_processing_sqs_message",
            "error_type" => error_type::PARSER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("sqs_message_processing_failed_total", 1);
    }
}

#[cfg(feature = "sources-aws_s3")]
#[derive(Debug)]
pub struct SqsMessageDeleteSucceeded {
    pub message_ids: Vec<DeleteMessageBatchResultEntry>,
}

#[cfg(feature = "sources-aws_s3")]
impl InternalEvent for SqsMessageDeleteSucceeded {
    fn emit_logs(&self) {
        trace!(message = "Deleted SQS message(s).",
            message_ids = %self.message_ids.iter()
                .map(|x| x.id.to_string())
                .collect::<Vec<_>>()
                .join(", "));
    }

    fn emit_metrics(&self) {
        counter!(
            "sqs_message_delete_succeeded_total",
            self.message_ids.len() as u64
        );
    }
}

#[cfg(feature = "sources-aws_s3")]
#[derive(Debug)]
pub struct SqsMessageDeletePartialError {
    pub entries: Vec<BatchResultErrorEntry>,
}

#[cfg(feature = "sources-aws_s3")]
impl InternalEvent for SqsMessageDeletePartialError {
    fn emit_logs(&self) {
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
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_deleting_some_sqs_messages",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("sqs_message_delete_failed_total", self.entries.len() as u64);
    }
}

#[cfg(feature = "sources-aws_s3")]
#[derive(Debug)]
pub struct SqsMessageDeleteBatchError {
    pub entries: Vec<DeleteMessageBatchRequestEntry>,
    pub error: RusotoError<DeleteMessageBatchError>,
}

#[cfg(feature = "sources-aws_s3")]
impl InternalEvent for SqsMessageDeleteBatchError {
    fn emit_logs(&self) {
        error!(
            message = "Deletion of SQS message(s) failed.",
            message_ids = %self.entries.iter()
                .map(|x| x.id.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            error = %self.error,
            error_code = "failed_deleting_all_sqs_messages",
            error_type = error_type::ACKNOWLEDGMENT_FAILED,
            stage = error_stage::PROCESSING,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => "failed_deleting_all_sqs_messages",
            "error_type" => error_type::ACKNOWLEDGMENT_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("sqs_message_delete_failed_total", self.entries.len() as u64);
        counter!("sqs_message_delete_batch_failed_total", 1);
    }
}

#[derive(Debug)]
pub struct SqsS3EventRecordInvalidEventIgnored<'a> {
    pub bucket: &'a str,
    pub key: &'a str,
    pub kind: &'a str,
    pub name: &'a str,
}

impl<'a> InternalEvent for SqsS3EventRecordInvalidEventIgnored<'a> {
    fn emit_logs(&self) {
        warn!(message = "Ignored S3 record in SQS message for an event that was not ObjectCreated.",
            bucket = %self.bucket, key = %self.key, kind = %self.kind, name = %self.name);
    }

    fn emit_metrics(&self) {
        counter!("sqs_s3_event_record_ignored_total", 1, "ignore_type" => "invalid_event_kind");
    }
}
