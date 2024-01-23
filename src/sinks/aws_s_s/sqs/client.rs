use super::{Client, SendMessageEntry, SendMessageResponse};
use aws_sdk_sqs::operation::send_message::SendMessageError;
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use futures::TryFutureExt;
use tracing::Instrument;

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

#[async_trait::async_trait]
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
