use std::task::{Context, Poll};

use futures::{future::BoxFuture, TryFutureExt};
use rusoto_core::RusotoError;
use rusoto_sqs::{SendMessageError, SendMessageRequest, Sqs, SqsClient};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::event::EventStatus;
use vector_core::internal_event::EventsSent;
use vector_core::stream::DriverResponse;
use vector_core::ByteSizeOf;

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
    type Error = RusotoError<SendMessageError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, entry: SendMessageEntry) -> Self::Future {
        let byte_size = entry.size_of();

        let client = self.client.clone();
        let request = SendMessageRequest {
            message_body: entry.message_body,
            message_group_id: entry.message_group_id,
            message_deduplication_id: entry.message_deduplication_id,
            queue_url: entry.queue_url,
            ..Default::default()
        };

        Box::pin(async move {
            client
                .send_message(request)
                .map_ok(|_| SendMessageResponse { byte_size })
                .instrument(info_span!("request"))
                .await
        })
    }
}

pub(crate) struct SendMessageResponse {
    byte_size: usize,
}

impl DriverResponse for SendMessageResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: 1,
            byte_size: self.byte_size,
            output: None,
        }
    }
}
