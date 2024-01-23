use std::task::{Context, Poll};

use futures_util::future::BoxFuture;
use tower::Service;
use vector_lib::stream::DriverResponse;
use vector_lib::{
    finalization::{EventFinalizers, EventStatus, Finalizable},
    request_metadata::{GroupedCountByteSize, MetaDescriptive, RequestMetadata},
};

/// Generalized request for sending metrics to a StatsD endpoint.
#[derive(Clone, Debug)]
pub struct StatsdRequest {
    pub payload: Vec<u8>,
    pub finalizers: EventFinalizers,
    pub metadata: RequestMetadata,
}

impl Finalizable for StatsdRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for StatsdRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

// Placeholder response to shuttle request metadata for StatsD requests.
//
// As StatsD sends no response back to a caller, there's no success/failure to report except for raw
// I/O errors when sending the request. Primarily, this type shuttles the metadata around the
// request -- events sent, bytes sent, etc -- that is required by `Driver`.
#[derive(Debug)]
pub struct StatsdResponse {
    metadata: RequestMetadata,
}

impl DriverResponse for StatsdResponse {
    fn event_status(&self) -> EventStatus {
        // If we generated a response, that implies our send concluded without any I/O errors, so we
        // assume things were delivered.
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        self.metadata.events_estimated_json_encoded_byte_size()
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.metadata.request_encoded_size())
    }
}

#[derive(Clone)]
pub struct StatsdService<T> {
    transport: T,
}

impl<T> StatsdService<T> {
    /// Creates a new `StatsdService` with the given `transport` service.
    ///
    /// The `transport` service is responsible for sending the actual encoded requests to the downstream
    /// endpoint.
    pub const fn from_transport(transport: T) -> Self {
        Self { transport }
    }
}

impl<T> Service<StatsdRequest> for StatsdService<T>
where
    T: Service<Vec<u8>>,
    T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    T::Future: Send + 'static,
{
    type Response = StatsdResponse;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.transport.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, request: StatsdRequest) -> Self::Future {
        let StatsdRequest {
            payload,
            finalizers: _,
            metadata,
        } = request;

        let send_future = self.transport.call(payload);

        Box::pin(async move {
            send_future
                .await
                .map(|_| StatsdResponse { metadata })
                .map_err(Into::into)
        })
    }
}
