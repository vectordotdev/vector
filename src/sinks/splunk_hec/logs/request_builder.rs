use vector_core::{
    event::{EventFinalizers, Finalizable},
    ByteSizeOf,
};

use crate::sinks::util::{
    encoding::{Encoder, EncodingConfig},
    Compression, RequestBuilder,
};

use super::{encoder::HecLogsEncoder, service::HecLogsRequest, sink::ProcessedEvent};

pub struct HecLogsRequestBuilder {
    pub compression: Compression,
    pub encoding: EncodingConfig<HecLogsEncoder>,
}

impl RequestBuilder<Vec<ProcessedEvent>> for HecLogsRequestBuilder {
    type Metadata = (usize, usize, EventFinalizers);
    type Events = Vec<ProcessedEvent>;
    type Encoder = EncodingConfig<HecLogsEncoder>;
    type Payload = Vec<u8>;
    type Request = HecLogsRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(&self, input: Vec<ProcessedEvent>) -> (Self::Metadata, Self::Events) {
        let mut events = input;
        let finalizers = events.take_finalizers();
        let events_byte_size: usize = events.iter().map(|e| e.log.size_of()).sum();

        ((events.len(), events_byte_size, finalizers), events)
    }

    fn encode_events(&self, events: Self::Events) -> Result<Self::Payload, Self::Error> {
        let mut payload = Vec::new();
        self.encoding.encode_input(events, &mut payload)?;
        Ok(payload)
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (events_count, events_byte_size, finalizers) = metadata;
        HecLogsRequest {
            body: payload,
            finalizers,
            events_count,
            events_byte_size,
        }
    }
}
