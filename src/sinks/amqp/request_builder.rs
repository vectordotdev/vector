//! Request builder for the `AMQP` sink.
//! Responsible for taking the event (which includes rendered template values) and turning
//! it into the raw bytes and other data needed to send the request to `AMQP`.
use crate::sinks::prelude::*;
use bytes::Bytes;
use lapin::BasicProperties;
use std::io;

use super::{encoder::AmqpEncoder, service::AmqpRequest, sink::AmqpEvent};

pub(super) struct AmqpMetadata {
    exchange: String,
    routing_key: String,
    properties: BasicProperties,
    finalizers: EventFinalizers,
}

/// Build the request to send to `AMQP` by using the encoder to convert it into
/// raw bytes and pass along the resolved template fields to determine where to
/// route the event.
pub(super) struct AmqpRequestBuilder {
    pub(super) encoder: AmqpEncoder,
}

impl RequestBuilder<AmqpEvent> for AmqpRequestBuilder {
    type Metadata = AmqpMetadata;
    type Events = Event;
    type Encoder = AmqpEncoder;
    type Payload = Bytes;
    type Request = AmqpRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        mut input: AmqpEvent,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let builder = RequestMetadataBuilder::from_event(&input.event);

        let metadata = AmqpMetadata {
            exchange: input.exchange,
            routing_key: input.routing_key,
            properties: input.properties,
            finalizers: input.event.take_finalizers(),
        };

        (metadata, builder, input.event)
    }

    fn build_request(
        &self,
        amqp_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let body = payload.into_payload();
        AmqpRequest::new(
            body,
            amqp_metadata.exchange,
            amqp_metadata.routing_key,
            amqp_metadata.properties,
            amqp_metadata.finalizers,
            metadata,
        )
    }
}
