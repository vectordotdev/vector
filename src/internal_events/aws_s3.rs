#[cfg(feature = "sources-aws_s3")]
pub mod source {
    use crate::internal_events::InternalEvent;
    use crate::sources::aws_s3::sqs::ProcessingError;
    use metrics::counter;
    use rusoto_core::RusotoError;
    use rusoto_sqs::{
        BatchResultErrorEntry, DeleteMessageBatchError, DeleteMessageBatchRequestEntry,
        DeleteMessageBatchResultEntry, ReceiveMessageError,
    };

    #[derive(Debug)]
    pub(crate) struct SqsS3EventReceived {
        pub byte_size: usize,
    }

    impl InternalEvent for SqsS3EventReceived {
        fn emit_metrics(&self) {
            counter!("events_in_total", 1);
            counter!("processed_bytes_total", self.byte_size as u64);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SqsMessageReceiveFailed<'a> {
        pub error: &'a RusotoError<ReceiveMessageError>,
    }

    impl<'a> InternalEvent for SqsMessageReceiveFailed<'a> {
        fn emit_logs(&self) {
            warn!(message = "Failed to fetch SQS events.", error = %self.error);
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
            trace!(message = "Received SQS messages.", count = %self.count);
        }

        fn emit_metrics(&self) {
            counter!("sqs_message_receive_succeeded_total", 1);
            counter!("sqs_message_received_messages_total", self.count as u64);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SqsMessageProcessingSucceeded<'a> {
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

    #[derive(Debug)]
    pub(crate) struct SqsMessageProcessingFailed<'a> {
        pub message_id: &'a str,
        pub error: &'a ProcessingError,
    }

    impl<'a> InternalEvent for SqsMessageProcessingFailed<'a> {
        fn emit_logs(&self) {
            warn!(message = "Failed to process SQS message.", message_id = %self.message_id, error = %self.error);
        }

        fn emit_metrics(&self) {
            counter!("sqs_message_processing_failed_total", 1);
        }
    }

    #[derive(Debug)]
    pub(crate) struct SqsMessageDeleteSucceeded {
        pub message_ids: Vec<DeleteMessageBatchResultEntry>,
    }

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

    #[derive(Debug)]
    enum MessageDeleteFailureState {
        Complete(
            Vec<DeleteMessageBatchRequestEntry>,
            RusotoError<DeleteMessageBatchError>,
        ),
        Partial(Vec<BatchResultErrorEntry>),
    }

    #[derive(Debug)]
    pub(crate) struct SqsMessageDeleteFailed {
        state: MessageDeleteFailureState,
    }

    impl SqsMessageDeleteFailed {
        pub(crate) fn complete(
            entries: Vec<DeleteMessageBatchRequestEntry>,
            error: RusotoError<DeleteMessageBatchError>,
        ) -> Self {
            SqsMessageDeleteFailed {
                state: MessageDeleteFailureState::Complete(entries, error),
            }
        }

        pub(crate) fn partial(entries: Vec<BatchResultErrorEntry>) -> Self {
            SqsMessageDeleteFailed {
                state: MessageDeleteFailureState::Partial(entries),
            }
        }
    }

    impl InternalEvent for SqsMessageDeleteFailed {
        fn emit_logs(&self) {
            match self.state {
                MessageDeleteFailureState::Complete(ref entries, ref error) => {
                    warn!(message = "Deletion of SQS message(s) failed.",
                        %error,
                        message_ids = %entries.iter()
                            .map(|x| x.id.to_string())
                            .collect::<Vec<_>>()
                            .join(", "));
                }
                MessageDeleteFailureState::Partial(ref entries) => {
                    warn!(message = "Deletion of SQS message(s) failed.",
                        message_ids = %entries.iter()
                            .map(|x| format!("{}/{}", x.id, x.code))
                            .collect::<Vec<_>>()
                            .join(", "));
                }
            }
        }

        fn emit_metrics(&self) {
            match self.state {
                MessageDeleteFailureState::Complete(ref entries, _) => {
                    counter!("sqs_message_delete_failed_total", entries.len() as u64);
                }
                MessageDeleteFailureState::Partial(ref entries) => {
                    counter!("sqs_message_delete_failed_total", entries.len() as u64);
                }
            }
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
            warn!(message = "Ignored S3 record in SQS message for an event that was not ObjectCreated.",
                bucket = %self.bucket, key = %self.key, kind = %self.kind, name = %self.name);
        }

        fn emit_metrics(&self) {
            counter!("sqs_s3_event_record_ignored_total", 1, "ignore_type" => "invalid_event_kind");
        }
    }
}
