use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::task::{Context, Poll};

use azure_messaging_eventhubs::ProducerClient;
use bytes::Bytes;
use vector_lib::config;

use crate::sinks::prelude::*;

pub struct AzureEventHubsRequest {
    pub body: Bytes,
    pub metadata: AzureEventHubsRequestMetadata,
    pub request_metadata: RequestMetadata,
}

pub struct AzureEventHubsRequestMetadata {
    pub finalizers: EventFinalizers,
}

pub struct AzureEventHubsResponse {
    event_byte_size: GroupedCountByteSize,
    raw_byte_size: usize,
    event_status: EventStatus,
}

impl DriverResponse for AzureEventHubsResponse {
    fn event_status(&self) -> EventStatus {
        self.event_status
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.event_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.raw_byte_size)
    }
}

impl Finalizable for AzureEventHubsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl MetaDescriptive for AzureEventHubsRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

#[derive(Clone)]
pub struct AzureEventHubsService {
    producer: Arc<ProducerClient>,
    in_flight: Arc<AtomicUsize>,
    max_in_flight: usize,
}

impl AzureEventHubsService {
    pub fn new(producer: ProducerClient, max_in_flight: usize) -> Self {
        Self {
            producer: Arc::new(producer),
            in_flight: Arc::new(AtomicUsize::new(0)),
            max_in_flight,
        }
    }
}

impl Service<AzureEventHubsRequest> for AzureEventHubsService {
    type Response = AzureEventHubsResponse;
    type Error = String;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.in_flight.load(Ordering::Relaxed) >= self.max_in_flight {
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn call(&mut self, request: AzureEventHubsRequest) -> Self::Future {
        let producer = Arc::clone(&self.producer);
        let in_flight = Arc::clone(&self.in_flight);
        let raw_byte_size = request.body.len();
        let event_byte_size = request
            .request_metadata
            .into_events_estimated_json_encoded_byte_size();

        in_flight.fetch_add(1, Ordering::Relaxed);

        Box::pin(async move {
            let _guard = InFlightGuard(in_flight);

            let event_data = azure_messaging_eventhubs::models::EventData::builder()
                .with_body(request.body.to_vec())
                .build();

            match producer.send_event(event_data, None).await {
                Ok(_) => Ok(AzureEventHubsResponse {
                    event_byte_size,
                    raw_byte_size,
                    event_status: EventStatus::Delivered,
                }),
                Err(e) => {
                    error!(message = "Failed to send event to Event Hubs.", error = %e);
                    Ok(AzureEventHubsResponse {
                        event_byte_size: config::telemetry().create_request_count_byte_size(),
                        raw_byte_size: 0,
                        event_status: EventStatus::Errored,
                    })
                }
            }
        })
    }
}

/// RAII guard that decrements in-flight counter on drop.
struct InFlightGuard(Arc<AtomicUsize>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}
