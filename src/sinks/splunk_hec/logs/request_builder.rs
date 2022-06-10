use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use vector_core::event::{EventFinalizers, Finalizable};

use super::{
    encoder::HecLogsEncoder,
    sink::{HecProcessedEvent, Partitioned},
};
use crate::sinks::{
    splunk_hec::common::request::HecRequest,
    util::{request_builder::EncodeResult, Compression, RequestBuilder},
};

pub struct HecLogsRequestBuilder {
    pub encoder: HecLogsEncoder,
    pub compression: Compression,
}

#[derive(Debug, Clone)]
pub struct RequestMetadata {
    events_count: usize,
    events_byte_size: usize,
    finalizers: EventFinalizers,
    partition: Option<Arc<str>>,
    metadata: Option<HashMap<String, String>>,
}

impl RequestBuilder<(Option<Partitioned>, Vec<HecProcessedEvent>)> for HecLogsRequestBuilder {
    type Metadata = RequestMetadata;
    type Events = Vec<HecProcessedEvent>;
    type Encoder = HecLogsEncoder;
    type Payload = Bytes;
    type Request = HecRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (Option<Partitioned>, Vec<HecProcessedEvent>),
    ) -> (Self::Metadata, Self::Events) {
        let (partition, mut events) = input;
        let (partition, metadata) = match partition {
            None => (None, None),
            Some(partition) => {
                let (p, m) = partition.into_parts();
                (p, Some(m))
            }
        };

        let finalizers = events.take_finalizers();
        let events_byte_size: usize = events.iter().map(|e| e.metadata.event_byte_size).sum();

        (
            RequestMetadata {
                events_count: events.len(),
                events_byte_size,
                finalizers,
                partition,
                metadata,
            },
            events,
        )
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        HecRequest {
            body: payload.into_payload(),
            finalizers: metadata.finalizers,
            events_count: metadata.events_count,
            events_byte_size: metadata.events_byte_size,
            passthrough_token: metadata.partition,
            metadata: metadata.metadata,
        }
    }
}
