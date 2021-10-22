use vector_core::event::{EventFinalizers, Finalizable, LogEvent};

use crate::sinks::util::{
    encoding::EncodingConfig, processed_event::ProcessedEvent, Compression, RequestBuilder,
};

use super::{
    encoder::HecLogsEncoder, service::HecLogsRequest, sink::HecLogsProcessedEventMetadata,
};

pub struct HecLogsRequestBuilder {
    pub compression: Compression,
    pub encoding: EncodingConfig<HecLogsEncoder>,
}

impl RequestBuilder<Vec<ProcessedEvent<LogEvent, HecLogsProcessedEventMetadata>>>
    for HecLogsRequestBuilder
{
    type Metadata = (usize, usize, EventFinalizers);
    type Events = Vec<ProcessedEvent<LogEvent, HecLogsProcessedEventMetadata>>;
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

    fn split_input(
        &self,
        input: Vec<ProcessedEvent<LogEvent, HecLogsProcessedEventMetadata>>,
    ) -> (Self::Metadata, Self::Events) {
        let mut events = input;
        let finalizers = events.take_finalizers();
        let events_byte_size: usize = events.iter().map(|e| e.metadata.event_byte_size).sum();

        ((events.len(), events_byte_size, finalizers), events)
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
