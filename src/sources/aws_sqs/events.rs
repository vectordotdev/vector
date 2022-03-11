use aws_sdk_sqs::{error::DeleteMessageBatchError, types::SdkError};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsSqsBytesReceived {
    pub byte_size: usize,
}

impl InternalEvent for AwsSqsBytesReceived {
    fn emit(self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = "http",
        );
        counter!("component_received_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct SqsMessageDeleteError<'a> {
    pub error: &'a SdkError<DeleteMessageBatchError>,
}

impl<'a> InternalEvent for SqsMessageDeleteError<'a> {
    fn emit(self) {
        error!(message = "Failed to delete SQS events.", error = %self.error);
        counter!("sqs_message_delete_failed_total", 1);
    }
}
