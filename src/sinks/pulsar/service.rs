use std::collections::HashMap;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use pulsar::producer::Message;
use pulsar::{Error as PulsarError, Executor, MultiTopicProducer, ProducerOptions, Pulsar};
use tokio::sync::Mutex;

use crate::internal_events::PulsarSendingError;
use crate::sinks::{prelude::*, pulsar::request_builder::PulsarMetadata};

#[derive(Clone)]
pub(super) struct PulsarRequest {
    pub body: Bytes,
    pub metadata: PulsarMetadata,
    pub request_metadata: RequestMetadata,
}

pub struct PulsarResponse {
    byte_size: usize,
    event_byte_size: GroupedCountByteSize,
}

impl DriverResponse for PulsarResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.event_byte_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}

impl Finalizable for PulsarRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.metadata.finalizers)
    }
}

impl MetaDescriptive for PulsarRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.request_metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.request_metadata
    }
}

pub struct PulsarService<Exe: Executor> {
    // NOTE: the reason for the Mutex here is because the `Producer` from the pulsar crate
    // needs to be `mut`, and the `Service::call()` returns a Future.
    producer: Arc<Mutex<MultiTopicProducer<Exe>>>,
}

impl<Exe: Executor> PulsarService<Exe> {
    pub(crate) fn new(
        pulsar_client: Pulsar<Exe>,
        producer_options: ProducerOptions,
        producer_name: Option<String>,
    ) -> PulsarService<Exe> {
        let mut builder = pulsar_client.producer().with_options(producer_options);

        if let Some(name) = producer_name {
            builder = builder.with_name(name);
        }

        let producer = builder.build_multi_topic();

        PulsarService {
            producer: Arc::new(Mutex::new(producer)),
        }
    }
}

impl<Exe: Executor> Service<PulsarRequest> for PulsarService<Exe> {
    type Response = PulsarResponse;
    type Error = PulsarError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.producer.try_lock() {
            Ok(_) => Poll::Ready(Ok(())),
            Err(_) => Poll::Pending,
        }
    }

    fn call(&mut self, request: PulsarRequest) -> Self::Future {
        let producer = Arc::clone(&self.producer);
        let topic = request.metadata.topic.clone();
        let event_time = request
            .metadata
            .timestamp_millis
            .to_owned()
            .map(|t| t as u64);

        Box::pin(async move {
            let body = request.body.clone();
            let byte_size = request.body.len();

            let mut properties = HashMap::new();
            if let Some(props) = request.metadata.properties {
                for (key, value) in props {
                    properties.insert(key.into(), String::from_utf8_lossy(&value).to_string());
                }
            }

            let partition_key = request
                .metadata
                .key
                .map(|key| String::from_utf8_lossy(&key).to_string());

            let message = Message {
                payload: body.as_ref().to_vec(),
                properties,
                partition_key,
                event_time,
                ..Default::default()
            };

            // The locking if this mutex is not normal in `Service::call()` implementations, but we
            // at least can limit the scope of the lock by placing it here, and reduce the
            // possibility of performance impact by checking the `try_lock()` result in
            // `poll_ready()`. This sink is already limited to sequential request handling due to
            // the pulsar API, so this shouldn't impact performance from a concurrent requests
            // standpoint.
            let fut = producer
                .lock()
                .await
                .send_non_blocking(topic, message)
                .await;

            match fut {
                Ok(resp) => match resp.await {
                    Ok(_) => Ok(PulsarResponse {
                        byte_size,
                        event_byte_size: request
                            .request_metadata
                            .into_events_estimated_json_encoded_byte_size(),
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
