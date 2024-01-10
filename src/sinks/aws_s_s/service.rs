use std::marker::PhantomData;
use std::task::{Context, Poll};

use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::result::SdkError;
use futures::future::BoxFuture;
use tower::Service;
use vector_lib::request_metadata::GroupedCountByteSize;
use vector_lib::stream::DriverResponse;
use vector_lib::{event::EventStatus, ByteSizeOf};

use super::{client::Client, request_builder::SendMessageEntry};

pub(super) struct SSService<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    client: C,
    _phantom: PhantomData<fn() -> E>,
}

impl<C, E> SSService<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    pub(super) const fn new(client: C) -> Self {
        Self {
            client,
            _phantom: PhantomData,
        }
    }
}

impl<C, E> Clone for SSService<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    fn clone(&self) -> SSService<C, E> {
        SSService {
            client: self.client.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<C, E> Service<SendMessageEntry> for SSService<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    type Response = SendMessageResponse;
    type Error = SdkError<E, HttpResponse>;
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

pub(super) struct SendMessageResponse {
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
