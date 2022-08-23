use crate::internal_events::sink::{
    AMQPAcknowledgementError, AMQPDeliveryError, AMQPNoAcknowledgement,
};
use bytes::Bytes;
use futures::future::BoxFuture;
use lapin::{options::BasicPublishOptions, BasicProperties};
use snafu::Snafu;
use std::{
    sync::Arc,
    task::{Context, Poll},
};
use tower::Service;
use vector_common::{
    finalization::{EventFinalizers, EventStatus, Finalizable},
    internal_event::{BytesSent, EventsSent},
};
use vector_core::stream::DriverResponse;

/// The request contains the data to send to `AMQP` together
/// with the information need to route the message.
pub(super) struct AMQPRequest {
    body: Bytes,
    exchange: String,
    routing_key: String,
    finalizers: EventFinalizers,
}

impl AMQPRequest {
    pub(super) fn new(
        body: Bytes,
        exchange: String,
        routing_key: String,
        finalizers: EventFinalizers,
    ) -> Self {
        Self {
            body,
            exchange,
            routing_key,
            finalizers,
        }
    }
}

impl Finalizable for AMQPRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

/// A successful response from `AMQP`.
pub(super) struct AMQPResponse {
    byte_size: usize,
}

impl DriverResponse for AMQPResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: 1,
            byte_size: self.byte_size,
            output: None,
        }
    }

    fn bytes_sent(&self) -> Option<BytesSent> {
        Some(BytesSent {
            byte_size: self.byte_size,
            protocol: "amqp 0.9.1",
        })
    }
}

/// The tower service that handles the actual sending of data to `AMQP`.
pub(super) struct AMQPService {
    pub(super) channel: Arc<lapin::Channel>,
}

#[derive(Debug, Snafu)]
pub(super) enum AMQPError {
    #[snafu(display("Failed retrieving Acknowledgement: {}", error))]
    AMQPAcknowledgementFailed { error: lapin::Error },

    #[snafu(display("Failed AMQP request: {}", error))]
    AMQPDeliveryFailed { error: lapin::Error },
}

impl Service<AMQPRequest> for AMQPService {
    type Response = AMQPResponse;

    type Error = AMQPError;

    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: AMQPRequest) -> Self::Future {
        let channel = Arc::clone(&self.channel);

        Box::pin(async move {
            channel
                .confirm_select(lapin::options::ConfirmSelectOptions::default())
                .await
                .unwrap();

            let byte_size = req.body.len();
            let f = channel
                .basic_publish(
                    &req.exchange,
                    &req.routing_key,
                    BasicPublishOptions::default(),
                    req.body.as_ref(),
                    BasicProperties::default(),
                )
                .await;

            match f {
                Ok(result) => match result.await {
                    Ok(lapin::publisher_confirm::Confirmation::Nack(_)) => {
                        emit!(AMQPNoAcknowledgement::default());
                        Ok(AMQPResponse { byte_size })
                    }
                    Err(error) => {
                        // TODO: In due course the caller could emit these on error.
                        emit!(AMQPAcknowledgementError { error: &error });
                        Err(AMQPError::AMQPAcknowledgementFailed { error })
                    }
                    Ok(_) => Ok(AMQPResponse { byte_size }),
                },
                Err(error) => {
                    // TODO: In due course the caller could emit these on error.
                    emit!(AMQPDeliveryError { error: &error });
                    Err(AMQPError::AMQPDeliveryFailed { error })
                }
            }
        })
    }
}
