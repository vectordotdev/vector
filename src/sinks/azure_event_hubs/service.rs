use std::sync::Arc;
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
}

impl AzureEventHubsService {
    pub fn new(producer: ProducerClient) -> Self {
        Self {
            producer: Arc::new(producer),
        }
    }
}

impl Service<AzureEventHubsRequest> for AzureEventHubsService {
    type Response = AzureEventHubsResponse;
    type Error = String;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: AzureEventHubsRequest) -> Self::Future {
        let producer = Arc::clone(&self.producer);
        let raw_byte_size = request.body.len();
        let partition_id = request.partition_id;
        let event_byte_size = request
            .request_metadata
            .into_events_estimated_json_encoded_byte_size();

        Box::pin(async move {
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

#[cfg(test)]
mod tests {
    use super::*;

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
