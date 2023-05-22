use std::task::{Context, Poll};

use aws_sdk_sqs::{error::SendMessageError, types::SdkError, Client as SqsClient};
use futures::{future::BoxFuture, TryFutureExt};
use tower::Service;
use tracing::Instrument;
use vector_common::json_size::JsonSize;
use vector_core::{
    event::EventStatus, internal_event::CountByteSize, stream::DriverResponse, ByteSizeOf,
};

use super::request_builder::SendMessageEntry;

#[derive(Clone)]
pub(crate) struct SqsService {
    client: SqsClient,
}

impl SqsService {
    pub const fn new(client: SqsClient) -> Self {
        Self { client }
    }
}

impl Service<SendMessageEntry> for SqsService {
    type Response = SendMessageResponse;
    type Error = SdkError<SendMessageError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    // Emission of an internal event in case of errors is handled upstream by the caller.
    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Emission of internal events for errors and dropped events is handled upstream by the caller.
    fn call(&mut self, entry: SendMessageEntry) -> Self::Future {
        let byte_size = entry.size_of();
        let client = self.client.clone();

        Box::pin(async move {
            client
                .send_message()
                .message_body(entry.message_body)
                .set_message_group_id(entry.message_group_id)
                .set_message_deduplication_id(entry.message_deduplication_id)
                .queue_url(entry.queue_url)
                .send()
                .map_ok(|_| SendMessageResponse {
                    byte_size,
                    json_byte_size: entry.metadata.events_estimated_json_encoded_byte_size(),
                })
                .instrument(info_span!("request").or_current())
                .await
        })
    }
}

pub(crate) struct SendMessageResponse {
    byte_size: usize,
    json_byte_size: JsonSize,
}

impl DriverResponse for SendMessageResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(1, self.json_byte_size)
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}
