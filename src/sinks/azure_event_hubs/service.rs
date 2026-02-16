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
    pub partition_id: Option<String>,
    pub metadata: AzureEventHubsRequestMetadata,
    pub request_metadata: RequestMetadata,
}

pub struct AzureEventHubsRequestMetadata {
    pub finalizers: EventFinalizers,
    pub partition_id: Option<String>,
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
        let partition_id = request.partition_id;
        let event_byte_size = request
            .request_metadata
            .into_events_estimated_json_encoded_byte_size();

        in_flight.fetch_add(1, Ordering::Relaxed);

        Box::pin(async move {
            let _guard = InFlightGuard(in_flight);

            let event_data = azure_messaging_eventhubs::models::EventData::builder()
                .with_body(request.body.to_vec())
                .build();

            let options = partition_id.map(|pid| {
                azure_messaging_eventhubs::SendEventOptions {
                    partition_id: Some(pid),
                }
            });

            match producer.send_event(event_data, options).await {
                Ok(_) => Ok(AzureEventHubsResponse {
                    event_byte_size,
                    raw_byte_size,
                    event_status: EventStatus::Delivered,
                }),
                Err(e) => {
                    emit!(crate::internal_events::azure_event_hubs::sink::AzureEventHubsSendError {
                        error: e.to_string(),
                    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_flight_guard_decrements_on_drop() {
        let counter = Arc::new(AtomicUsize::new(5));
        {
            let _guard = InFlightGuard(Arc::clone(&counter));
            assert_eq!(counter.load(Ordering::Relaxed), 5);
        }
        // After guard is dropped, counter should be decremented
        assert_eq!(counter.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn in_flight_guard_multiple_guards() {
        let counter = Arc::new(AtomicUsize::new(0));
        counter.fetch_add(3, Ordering::Relaxed);

        let g1 = InFlightGuard(Arc::clone(&counter));
        let g2 = InFlightGuard(Arc::clone(&counter));
        assert_eq!(counter.load(Ordering::Relaxed), 3);

        drop(g1);
        assert_eq!(counter.load(Ordering::Relaxed), 2);

        drop(g2);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn response_driver_response_delivered() {
        let response = AzureEventHubsResponse {
            event_byte_size: config::telemetry().create_request_count_byte_size(),
            raw_byte_size: 42,
            event_status: EventStatus::Delivered,
        };
        assert_eq!(response.event_status(), EventStatus::Delivered);
        assert_eq!(response.bytes_sent(), Some(42));
    }

    #[test]
    fn response_driver_response_errored() {
        let response = AzureEventHubsResponse {
            event_byte_size: config::telemetry().create_request_count_byte_size(),
            raw_byte_size: 0,
            event_status: EventStatus::Errored,
        };
        assert_eq!(response.event_status(), EventStatus::Errored);
        assert_eq!(response.bytes_sent(), Some(0));
    }

    #[test]
    fn request_finalizable() {
        let mut request = AzureEventHubsRequest {
            body: Bytes::from("test"),
            partition_id: None,
            metadata: AzureEventHubsRequestMetadata {
                finalizers: EventFinalizers::default(),
                partition_id: None,
            },
            request_metadata: RequestMetadata::default(),
        };
        let _finalizers = request.take_finalizers();
    }

    #[test]
    fn request_meta_descriptive() {
        let request = AzureEventHubsRequest {
            body: Bytes::from("test"),
            partition_id: Some("0".to_string()),
            metadata: AzureEventHubsRequestMetadata {
                finalizers: EventFinalizers::default(),
                partition_id: Some("0".to_string()),
            },
            request_metadata: RequestMetadata::default(),
        };
        let _meta = request.get_metadata();
    }
}
