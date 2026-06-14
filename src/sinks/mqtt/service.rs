use std::task::{Context, Poll};

use bytes::Bytes;
use futures::future::BoxFuture;
use rumqttc::v5::mqttbytes::v5::PublishProperties;
use snafu::Snafu;

use super::config::MqttQoS;
use crate::{common::mqtt::MqttClient, sinks::prelude::*};

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

pub(super) struct MqttService {
    pub(super) client: MqttClient,
    pub(super) quality_of_service: MqttQoS,
    pub(super) retain: bool,
    pub(super) publish_properties: Option<PublishProperties>,
}

#[derive(Debug, Snafu)]
pub(super) enum MqttError {
    #[snafu(display("MQTT v3.1.1 client error: {error}"))]
    V311Error { error: rumqttc::ClientError },
    #[snafu(display("MQTT v5 client error: {error}"))]
    V5Error { error: rumqttc::v5::ClientError },
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
        let byte_size = req.body.len();
        let json_size = req.metadata.into_events_estimated_json_encoded_byte_size();

        match &self.client {
            MqttClient::V311(client) => {
                let client = client.clone();
                Box::pin(async move {
                    client
                        .publish(&req.topic, quality_of_service.into(), retain, req.body)
                        .await
                        .map(|()| MqttResponse {
                            byte_size,
                            json_size,
                        })
                        .map_err(|error| MqttError::V311Error { error })
                })
            }
            MqttClient::V5(client) => {
                let client = client.clone();
                let properties = self.publish_properties.clone();
                Box::pin(async move {
                    let qos: rumqttc::v5::mqttbytes::QoS = quality_of_service.into();
                    let res = if let Some(props) = properties {
                        client
                            .publish_with_properties(&req.topic, qos, retain, req.body, props)
                            .await
                    } else {
                        client.publish(&req.topic, qos, retain, req.body).await
                    };

                    res.map(|()| MqttResponse {
                        byte_size,
                        json_size,
                    })
                    .map_err(|error| MqttError::V5Error { error })
                })
            }
        }
    }
}
