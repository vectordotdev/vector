use std::sync::Arc;
use std::task::{Context, Poll};

use azure_messaging_eventhubs::ProducerClient;
use bytes::Bytes;

use crate::sinks::prelude::*;

pub struct EventHubsRequest {
    pub body: Bytes,
    pub metadata: EventHubsRequestMetadata,
}

pub struct EventHubsRequestMetadata {
    pub finalizers: EventFinalizers,
    pub partition_id: Option<String>,
    pub event_hub_name: String,
}

pub struct EventHubsResponse {
    event_status: EventStatus,
}

impl EventHubsResponse {
    pub const fn event_status(&self) -> EventStatus {
        self.event_status
    }
}

#[derive(Clone)]
pub struct EventHubsService {
    producer: Arc<ProducerClient>,
    event_hub_name: String,
}

impl EventHubsService {
    pub const fn new(producer: Arc<ProducerClient>, event_hub_name: String) -> Self {
        Self {
            producer,
            event_hub_name,
        }
    }
}

impl Service<EventHubsRequest> for EventHubsService {
    type Response = EventHubsResponse;
    type Error = String;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: EventHubsRequest) -> Self::Future {
        let producer = Arc::clone(&self.producer);
        let event_hub_name = self.event_hub_name.clone();

        Box::pin(async move {
            let raw_byte_size = request.body.len();
            let partition_id = request.metadata.partition_id.clone();

            let event_data = azure_messaging_eventhubs::models::EventData::builder()
                .with_body(request.body.to_vec())
                .build();

            let pid_label = partition_id.as_deref().unwrap_or("").to_string();
            let options = partition_id.map(|pid| azure_messaging_eventhubs::SendEventOptions {
                partition_id: Some(pid),
            });

            match producer.send_event(event_data, options).await {
                Ok(_) => {
                    crate::internal_events::azure_event_hubs::sink::emit_eventhubs_sent_metrics(
                        1,
                        raw_byte_size,
                        &event_hub_name,
                        &pid_label,
                    );
                    Ok(EventHubsResponse {
                        event_status: EventStatus::Delivered,
                    })
                }
                Err(e) => {
                    emit!(
                        crate::internal_events::azure_event_hubs::sink::AzureEventHubsSendError {
                            error: e.to_string(),
                        }
                    );
                    Ok(EventHubsResponse {
                        event_status: EventStatus::Errored,
                    })
                }
            }
        })
    }
}
