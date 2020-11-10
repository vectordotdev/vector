#[cfg(feature = "sources-aws_s3")]
pub mod source {
    use crate::internal_events::InternalEvent;
    use crate::sources::aws_s3::sqs::ProcessingError;
    use metrics::counter;
    use rusoto_core::RusotoError;
    use rusoto_sqs::{DeleteMessageError, ReceiveMessageError};

    #[derive(Debug)]
    pub(crate) struct SqsMessageReceiveFailed<'a> {
        pub error: &'a RusotoError<ReceiveMessageError>,
    }

    impl<'a> InternalEvent for SqsMessageReceiveFailed<'a> {
        fn emit_logs(&self) {
            warn!(message = "Failed to fetch SQS events.", %self.error);
        }

        fn emit_metrics(&self) {
            counter!("sqs_message_receive_failed_total", 1);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SqsMessageReceiveSucceeded {
        pub count: usize,
    }

    impl InternalEvent for SqsMessageReceiveSucceeded {
        fn emit_logs(&self) {
            trace!(message = "Received SQS messages.", %self.count);
        }

        fn emit_metrics(&self) {
            counter!("sqs_message_receive_succeeded_total", 1);
            counter!("sqs_message_received_messages", self.count as u64,);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SqsMessageProcessingSucceeded<'a> {
        pub message_id: &'a str,
    }

    impl<'a> InternalEvent for SqsMessageProcessingSucceeded<'a> {
        fn emit_logs(&self) {
            trace!(message = "Processed SQS message succeededly.", %self.message_id);
        }

        fn emit_metrics(&self) {
            counter!("sqs_message_processing_succeeded_total", 1);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SqsMessageProcessingFailed<'a> {
        pub message_id: &'a str,
        pub error: &'a ProcessingError,
    }

    impl<'a> InternalEvent for SqsMessageProcessingFailed<'a> {
        fn emit_logs(&self) {
            warn!(message = "Failed to process SQS.", %self.message_id, %self.error);
        }

        fn emit_metrics(&self) {
            counter!("sqs_message_processing_failed_total", 1);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SqsMessageDeleteSucceeded<'a> {
        pub message_id: &'a str,
    }

    impl<'a> InternalEvent for SqsMessageDeleteSucceeded<'a> {
        fn emit_logs(&self) {
            trace!(message = "Deleted SQS message.", %self.message_id);
        }

        fn emit_metrics(&self) {
            counter!("sqs_message_delete_succeeded_total", 1);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SqsMessageDeleteFailed<'a> {
        pub message_id: &'a str,
        pub error: &'a RusotoError<DeleteMessageError>,
    }

    impl<'a> InternalEvent for SqsMessageDeleteFailed<'a> {
        fn emit_logs(&self) {
            warn!(message = "Deletion of SQS message failed.", %self.message_id, %self.error);
        }

        fn emit_metrics(&self) {
            counter!("sqs_message_delete_failed_total", 1);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SqsS3EventRecordInvalidEventIgnored<'a> {
        pub bucket: &'a str,
        pub key: &'a str,
        pub kind: &'a str,
        pub name: &'a str,
    }

    impl<'a> InternalEvent for SqsS3EventRecordInvalidEventIgnored<'a> {
        fn emit_logs(&self) {
            warn!(message = "Ignored S3 record in SQS message for an event that was not ObjectCreated.", %self.bucket, %self.key, %self.kind, %self.name);
        }

        fn emit_metrics(&self) {
            counter!("sqs_s3_event_record_ignored", 1, "ignore_type" => "invalid_event_kind");
        }
    }
}
