use aws_sdk_sqs::operation::send_message::SendMessageError;
use aws_sdk_sqs::operation::send_message_batch::SendMessageBatchError;
use aws_sdk_sqs::types::SendMessageBatchRequestEntry;
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use futures::TryFutureExt;
use tracing::Instrument;
use vector_lib::request_metadata::RequestMetadata;

use super::{Client, SendMessageEntry, SendMessageResponse};

#[derive(Clone, Debug)]
pub(super) struct SqsMessagePublisher {
    client: aws_sdk_sqs::Client,
    queue_url: String,
}

impl SqsMessagePublisher {
    pub(super) const fn new(client: aws_sdk_sqs::Client, queue_url: String) -> Self {
        Self { client, queue_url }
    }
}

impl Client<SendMessageError> for SqsMessagePublisher {
    async fn send_message(
        &self,
        entry: SendMessageEntry,
        byte_size: usize,
    ) -> Result<SendMessageResponse, SdkError<SendMessageError, HttpResponse>> {
        self.client
            .send_message()
            .message_body(entry.message_body)
            .set_message_group_id(entry.message_group_id)
            .set_message_deduplication_id(entry.message_deduplication_id)
            .queue_url(self.queue_url.clone())
            .send()
            .map_ok(|_| SendMessageResponse {
                byte_size,
                json_byte_size: entry
                    .metadata
                    .events_estimated_json_encoded_byte_size()
                    .clone(),
            })
            .instrument(info_span!("request").or_current())
            .await
    }
}

/// Batch request wrapper for send_message_batch API
#[derive(Debug, Clone)]
pub(super) struct BatchSendMessageRequest {
    pub(super) entries: Vec<SendMessageEntry>,
    pub(super) metadata: RequestMetadata,
}

impl vector_lib::ByteSizeOf for BatchSendMessageRequest {
    fn allocated_bytes(&self) -> usize {
        self.entries.allocated_bytes()
    }
}

impl crate::event::Finalizable for BatchSendMessageRequest {
    fn take_finalizers(&mut self) -> crate::event::EventFinalizers {
        self.entries.take_finalizers()
    }
}

impl vector_lib::request_metadata::MetaDescriptive for BatchSendMessageRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

/// Client for batched SQS operations using send_message_batch API
#[derive(Clone, Debug)]
pub(super) struct SqsBatchMessagePublisher {
    client: aws_sdk_sqs::Client,
    queue_url: String,
}

impl SqsBatchMessagePublisher {
    pub(super) const fn new(client: aws_sdk_sqs::Client, queue_url: String) -> Self {
        Self { client, queue_url }
    }

    pub(super) async fn send_message_batch(
        &self,
        request: BatchSendMessageRequest,
    ) -> Result<SendMessageResponse, SdkError<SendMessageBatchError, HttpResponse>> {
        let total_byte_size: usize = request.entries.iter().map(|e| e.message_body.len()).sum();

        let mut batch_request = self
            .client
            .send_message_batch()
            .queue_url(self.queue_url.clone());

        // Build batch entries
        for (idx, entry) in request.entries.iter().enumerate() {
            let batch_entry = SendMessageBatchRequestEntry::builder()
                .id(idx.to_string())
                .message_body(&entry.message_body)
                .set_message_group_id(entry.message_group_id.clone())
                .set_message_deduplication_id(entry.message_deduplication_id.clone())
                .build()
                .map_err(|e| SdkError::construction_failure(e))?;

            batch_request = batch_request.entries(batch_entry);
        }

        batch_request
            .send()
            .map_ok(move |response| {
                // Check for partial failures
                let failed = response.failed();
                if !failed.is_empty() {
                    warn!(
                        message = "Some messages failed in batch",
                        failed_count = failed.len(),
                        total_count = request.entries.len()
                    );
                    for failure in failed {
                        error!(
                            message = "Message failed in batch",
                            id = ?failure.id,
                            code = ?failure.code,
                            message = ?failure.message,
                            sender_fault = failure.sender_fault
                        );
                    }
                }

                SendMessageResponse {
                    byte_size: total_byte_size,
                    json_byte_size: request
                        .metadata
                        .events_estimated_json_encoded_byte_size()
                        .clone(),
                }
            })
            .instrument(info_span!("request").or_current())
            .await
    }
}
