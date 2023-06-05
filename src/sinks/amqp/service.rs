//! The main tower service that takes the request created by the request builder
//! and sends it to `AMQP`.
use crate::{
    internal_events::sink::{AmqpAcknowledgementError, AmqpDeliveryError},
    sinks::prelude::*,
};
use bytes::Bytes;
use futures::future::BoxFuture;
use lapin::{options::BasicPublishOptions, BasicProperties};
use snafu::Snafu;
use std::{
    sync::Arc,
    task::{Context, Poll},
};

/// The request contains the data to send to `AMQP` together
/// with the information need to route the message.
pub(super) struct AmqpRequest {
    body: Bytes,
    exchange: String,
    routing_key: String,
    properties: BasicProperties,
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
    event_json_size: JsonSize,
}

impl AmqpRequest {
    pub(super) fn new(
        body: Bytes,
        exchange: String,
        routing_key: String,
        properties: BasicProperties,
        finalizers: EventFinalizers,
        metadata: RequestMetadata,
        event_json_size: JsonSize,
    ) -> Self {
        Self {
            body,
            exchange,
            routing_key,
            properties,
            finalizers,
            metadata,
            event_json_size,
        }
    }
}

impl Finalizable for AmqpRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

impl MetaDescriptive for AmqpRequest {
    fn get_metadata(&self) -> RequestMetadata {
        self.metadata
    }
}

/// A successful response from `AMQP`.
pub(super) struct AmqpResponse {
    byte_size: usize,
    json_size: JsonSize,
}

impl DriverResponse for AmqpResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> CountByteSize {
        CountByteSize(1, self.json_size)
    }

    fn bytes_sent(&self) -> Option<usize> {
        Some(self.byte_size)
    }
}

/// The tower service that handles the actual sending of data to `AMQP`.
pub(super) struct AmqpService {
    pub(super) channel: Arc<lapin::Channel>,
}

#[derive(Debug, Snafu)]
pub(super) enum AmqpError {
    #[snafu(display("Failed retrieving Acknowledgement: {}", error))]
    AmqpAcknowledgementFailed { error: lapin::Error },

    #[snafu(display("Failed AMQP request: {}", error))]
    AmqpDeliveryFailed { error: lapin::Error },
}

impl Service<AmqpRequest> for AmqpService {
    type Response = AmqpResponse;

    type Error = AmqpError;

    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: AmqpRequest) -> Self::Future {
        let channel = Arc::clone(&self.channel);

        Box::pin(async move {
            channel
                .confirm_select(lapin::options::ConfirmSelectOptions::default())
                .await
                .unwrap();

            let byte_size = req.body.len();
            let fut = channel
                .basic_publish(
                    &req.exchange,
                    &req.routing_key,
                    BasicPublishOptions::default(),
                    req.body.as_ref(),
                    req.properties,
                )
                .await;

            match fut {
                Ok(result) => match result.await {
                    Ok(lapin::publisher_confirm::Confirmation::Nack(_)) => {
                        warn!("Received Negative Acknowledgement from AMQP server.");
                        Ok(AmqpResponse {
                            json_size: req.event_json_size,
                            byte_size,
                        })
                    }
                    Err(error) => {
                        // TODO: In due course the caller could emit these on error.
                        emit!(AmqpAcknowledgementError { error: &error });
                        Err(AmqpError::AmqpAcknowledgementFailed { error })
                    }
                    Ok(_) => Ok(AmqpResponse {
                        json_size: req.event_json_size,
                        byte_size,
                    }),
                },
                Err(error) => {
                    // TODO: In due course the caller could emit these on error.
                    emit!(AmqpDeliveryError { error: &error });
                    Err(AmqpError::AmqpDeliveryFailed { error })
                }
            }
        })
    }
}
