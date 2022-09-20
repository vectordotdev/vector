//! Request builder for the `AMQP` sink.
//! Responsible for taking the event (which includes rendered template values) and turning
//! it into the raw bytes and other data needed to send the request to `AMQP`.
use crate::{
    event::Event,
    sinks::util::{request_builder::EncodeResult, Compression, RequestBuilder},
};
use bytes::Bytes;
use std::io;
use vector_common::finalization::{EventFinalizers, Finalizable};

use super::{encoder::AmqpEncoder, service::AmqpRequest, sink::AmqpEvent};

pub(super) struct AmqpMetadata {
    exchange: String,
    routing_key: String,
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

    fn split_input(&self, mut input: AmqpEvent) -> (Self::Metadata, Self::Events) {
        let metadata = AmqpMetadata {
            exchange: input.exchange,
            routing_key: input.routing_key,
            finalizers: input.event.take_finalizers(),
        };

        (metadata, input.event)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let body = payload.into_payload();
        AmqpRequest::new(
            body,
            metadata.exchange,
            metadata.routing_key,
            metadata.finalizers,
        )
    }
}
