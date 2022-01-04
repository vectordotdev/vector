use std::sync::Arc;

use vector_core::event::{EventFinalizers, Finalizable};

use super::{encoder::HecLogsEncoder, sink::HecProcessedEvent};
use crate::sinks::{
    splunk_hec::common::request::HecRequest,
    util::{encoding::EncodingConfig, Compression, RequestBuilder},
};

pub struct HecLogsRequestBuilder {
    pub compression: Compression,
    pub encoding: EncodingConfig<HecLogsEncoder>,
}

impl RequestBuilder<(Option<Arc<str>>, Vec<HecProcessedEvent>)> for HecLogsRequestBuilder {
    type Metadata = (usize, usize, EventFinalizers, Option<Arc<str>>);
    type Events = Vec<HecProcessedEvent>;
    type Encoder = EncodingConfig<HecLogsEncoder>;
    type Payload = Vec<u8>;
    type Request = HecRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(
        &self,
        input: (Option<Arc<str>>, Vec<HecProcessedEvent>),
    ) -> (Self::Metadata, Self::Events) {
        let (passthrough_token, mut events) = input;
        let finalizers = events.take_finalizers();
        let events_byte_size: usize = events.iter().map(|e| e.metadata.event_byte_size).sum();

        (
            (
                events.len(),
                events_byte_size,
                finalizers,
                passthrough_token,
            ),
            events,
        )
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (events_count, events_byte_size, finalizers, passthrough_token) = metadata;
        HecRequest {
            body: payload,
            finalizers,
            events_count,
            events_byte_size,
            passthrough_token,
        }
    }
}
