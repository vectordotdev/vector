use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future::BoxFuture;
use pulsar::{Error as PulsarError, Executor, Producer, ProducerOptions, Pulsar};
use tokio::sync::Mutex;
use tower::Service;
use vector_common::internal_event::CountByteSize;
use vector_core::stream::DriverResponse;

use crate::event::{EventFinalizers, EventStatus, Finalizable};
use crate::internal_events::PulsarSendingError;
use crate::sinks::pulsar::request_builder::PulsarMetadata;
use crate::sinks::util::retries::RetryLogic;
use vector_common::request_metadata::{MetaDescriptive, RequestMetadata};

#[derive(Clone)]
pub(super) struct PulsarRequest {
    pub body: Bytes,
    pub metadata: PulsarMetadata,
    pub request_metadata: RequestMetadata,
}

pub struct PulsarResponse {
    event_byte_size: usize,
}

impl DriverResponse for PulsarResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(1, self.event_byte_size)
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.event_byte_size)
    }
}

impl Finalizable for PulsarRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl MetaDescriptive for PulsarRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.request_metadata
    }
}

/// Pulsar retry logic.
#[derive(Debug, Default, Clone)]
pub struct PulsarRetryLogic;

impl RetryLogic for PulsarRetryLogic {
    type Error = PulsarError;
    type Response = PulsarResponse;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        // TODO expand retry logic on different pulsar error types
        true
    }
}

#[derive(Clone)]
pub struct PulsarService<Exe: Executor> {
    // pulsar::Producer does not implement Clone
    producer: Arc<Mutex<Producer<Exe>>>,
}

impl<Exe: Executor> PulsarService<Exe> {
    pub(crate) async fn new(
        pulsar_client: Pulsar<Exe>,
        producer_options: ProducerOptions,
        producer_name: Option<String>,
        topic: &String,
    ) -> Result<PulsarService<Exe>, pulsar::Error> {
        let mut builder = pulsar_client
            .producer()
            .with_topic(topic)
            .with_options(producer_options);

        if let Some(name) = producer_name {
            builder = builder.with_name(name);
        }
        builder.build().await.map(|producer| PulsarService {
            producer: Arc::new(Mutex::new(producer)),
        })
    }
}

impl<Exe: Executor> Service<PulsarRequest> for PulsarService<Exe> {
    type Response = PulsarResponse;
    type Error = PulsarError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: PulsarRequest) -> Self::Future {
        let arc_producer = Arc::clone(&self.producer);
        let ts = request.metadata.timestamp_millis.to_owned();
        Box::pin(async move {
            let mut producer = arc_producer.lock().await;
            let body = request.body.clone();
            let mut msg_builder = producer.create_message().with_content(body.as_ref());
            if let Some(key) = request.metadata.key {
                msg_builder = msg_builder.with_key(String::from_utf8_lossy(&key));
            }
            if let Some(timestamp) = ts {
                msg_builder = msg_builder.event_time(timestamp as u64);
            }
            if let Some(properties) = request.metadata.properties {
                for (key, value) in properties {
                    msg_builder =
                        msg_builder.with_property(key, String::from_utf8_lossy(&value.clone()));
                }
            }

            match msg_builder.send().await {
                Ok(resp) => match resp.await {
                    Ok(_) => Ok(PulsarResponse {
                        event_byte_size: request.request_metadata.events_byte_size(),
                    }),
                    Err(e) => {
                        emit!(PulsarSendingError {
                            error: Box::new(PulsarError::Custom("failed to send".to_string())),
                            count: 1
                        });
                        Err(e)
                    }
                },
                Err(e) => {
                    emit!(PulsarSendingError {
                        error: Box::new(PulsarError::Custom("failed to send".to_string())),
                        count: 1,
                    });
                    Err(e)
                }
            }
        })
    }
}
