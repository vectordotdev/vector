use aws_sdk_sqs::{error::DeleteMessageBatchError, SdkError};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsSqsBytesReceived {
    pub byte_size: usize,
}

impl InternalEvent for AwsSqsBytesReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = "http",
        );
    }

    fn emit_metrics(&self) {
        counter!("component_received_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct SqsMessageDeleteError<'a> {
    pub error: &'a SdkError<DeleteMessageBatchError>,
}

impl<'a> InternalEvent for SqsMessageDeleteError<'a> {
    fn emit_logs(&self) {
        warn!(message = "Failed to delete SQS events.", error = %self.error);
    }

    fn emit_metrics(&self) {
        counter!("sqs_message_delete_failed_total", 1);
    }
}
