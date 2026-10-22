use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

use bytes::Bytes;
use futures::future::BoxFuture;
use rumqttc::{AsyncClient, ClientError, QoS};
use snafu::Snafu;

use super::config::MqttQoS;
use crate::sinks::prelude::*;

pub(super) struct MqttResponse {
    byte_size: usize,
    json_size: GroupedCountByteSize,
}

impl DriverResponse for MqttResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.json_size
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}

pub(super) struct MqttRequest {
    pub(super) body: Bytes,
    pub(super) topic: String,
    pub(super) finalizers: EventFinalizers,
    pub(super) metadata: RequestMetadata,
}

impl Finalizable for MqttRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for MqttRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

/// Keeps the request channel open and counts publishes until they leave the MQTT pipeline.
pub(super) struct MqttClient {
    client: AsyncClient,
    pending: AtomicUsize,
}

impl MqttClient {
    pub(super) const fn new(client: AsyncClient) -> Self {
        Self {
            client,
            pending: AtomicUsize::new(0),
        }
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) != 0
    }

    pub(super) fn complete(&self, count: usize) {
        if count != 0 {
            self.pending.fetch_sub(count, Ordering::Relaxed);
        }
    }

    pub(super) async fn publish(
        &self,
        topic: String,
        qos: QoS,
        retain: bool,
        body: Bytes,
    ) -> Result<(), ClientError> {
        // Count before sending: the event loop can process a request before send_async returns.
        self.pending.fetch_add(1, Ordering::Relaxed);
        let result = self.client.publish(topic, qos, retain, body).await;
        if result.is_err() {
            self.complete(1);
        }
        result
    }
}

pub(super) struct MqttService {
    pub(super) client: Arc<MqttClient>,
    pub(super) quality_of_service: MqttQoS,
    pub(super) retain: bool,
}

#[derive(Debug, Snafu)]
pub(super) enum MqttError {
    #[snafu(display("error"))]
    Error { error: ClientError },
}

impl Service<MqttRequest> for MqttService {
    type Response = MqttResponse;
    type Error = MqttError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: MqttRequest) -> Self::Future {
        let quality_of_service = self.quality_of_service;
        let retain = self.retain;
        let client = Arc::clone(&self.client);

        Box::pin(async move {
            let byte_size = req.body.len();

            let res = client
                .publish(req.topic, quality_of_service.into(), retain, req.body)
                .await;
            match res {
                Ok(()) => Ok(MqttResponse {
                    byte_size,
                    json_size: req.metadata.into_events_estimated_json_encoded_byte_size(),
                }),
                Err(error) => Err(MqttError::Error { error }),
            }
        })
    }
}
