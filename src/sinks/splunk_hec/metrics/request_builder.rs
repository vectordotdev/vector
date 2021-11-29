use vector_core::event::{EventFinalizers, Finalizable};

use crate::sinks::{
    splunk_hec::common::request::HecRequest,
    util::{Compression, RequestBuilder},
};

use super::{encoder::HecMetricsEncoder, sink::HecProcessedEvent};

pub struct HecMetricsRequestBuilder {
    pub compression: Compression,
}

impl RequestBuilder<Vec<HecProcessedEvent>> for HecMetricsRequestBuilder {
    type Metadata = (usize, usize, EventFinalizers);
    type Events = Vec<HecProcessedEvent>;
    type Encoder = HecMetricsEncoder;
    type Payload = Vec<u8>;
    type Request = HecRequest;
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
        HecRequest {
            body: payload,
            finalizers,
            events_count,
            events_byte_size,
            passthrough_token: None,
        }
    }
}
