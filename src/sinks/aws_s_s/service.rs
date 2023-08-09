use std::task::{Context, Poll};

use aws_sdk_sqs::{error::SendMessageError, types::SdkError};
use futures::future::BoxFuture;
use tower::Service;
use vector_common::request_metadata::GroupedCountByteSize;
use vector_core::{event::EventStatus, stream::DriverResponse, ByteSizeOf};

use super::{client::Client, request_builder::SendMessageEntry};

#[derive(Clone)]
pub(crate) struct SqsService<C>
where
    C: Client + Clone + Send + Sync + 'static,
{
    client: C,
}

impl<C> SqsService<C>
where
    C: Client + Clone + Send + Sync + 'static,
{
    pub const fn new(client: C) -> Self {
        Self { client }
    }
}

impl<C> Service<SendMessageEntry> for SqsService<C>
where
    C: Client + Clone + Send + Sync + 'static,
{
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

        Box::pin(async move { client.send_message(entry, byte_size).await })
    }
}

pub struct SendMessageResponse {
    pub(crate) byte_size: usize,
    pub(crate) json_byte_size: GroupedCountByteSize,
}

impl DriverResponse for SendMessageResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.json_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}
