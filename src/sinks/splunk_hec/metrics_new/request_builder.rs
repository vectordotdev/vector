use vector_core::{
    buffers::Ackable,
    event::{EventFinalizers, Finalizable},
    ByteSizeOf,
};

use crate::sinks::util::{Compression, ElementCount, RequestBuilder, encoding::EncodingConfig};

use super::{encoder::HecMetricsEncoder, sink::HecProcessedEvent};

pub struct HecMetricsRequest {
    body: Vec<u8>,
    finalizers: EventFinalizers,
    events_count: usize,
    events_byte_size: usize,
}

impl ByteSizeOf for HecMetricsRequest {
    fn allocated_bytes(&self) -> usize {
        self.body.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for HecMetricsRequest {
    fn element_count(&self) -> usize {
        self.events_count
    }
}

impl Ackable for HecMetricsRequest {
    fn ack_size(&self) -> usize {
        self.events_count
    }
}

impl Finalizable for HecMetricsRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}

pub struct HecMetricsRequestBuilder {
    pub compression: Compression,
}

impl RequestBuilder<Vec<HecProcessedEvent>> for HecMetricsRequestBuilder {
    type Metadata = (usize, usize, EventFinalizers);
    type Events = Vec<HecProcessedEvent>;
    type Encoder = HecMetricsEncoder;
    type Payload = Vec<u8>;
    type Request = HecMetricsRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &HecMetricsEncoder
    }

    fn split_input(&self, input: Vec<HecProcessedEvent>) -> (Self::Metadata, Self::Events) {
        let mut events = input;
        let finalizers = events.take_finalizers();
        let events_byte_size: usize = events.iter().map(|e| e.metadata.event_byte_size).sum();

        ((events.len(), events_byte_size, finalizers), events)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (events_count, events_byte_size, finalizers) = metadata;
        HecMetricsRequest {
            body: payload,
            finalizers,
            events_count,
            events_byte_size,
        }
    }
}
