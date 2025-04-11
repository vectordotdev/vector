//! `RequestBuilder` implementation for the `Doris` sink.

use super::sink::DorisPartitionKey;
use crate::sinks::prelude::*;
use bytes::Bytes;
use vector_lib::codecs::encoding::Framer;
use vector_lib::json_size::JsonSize;
use crate::sinks::doris::service_bak::DorisRequest;
use serde_json;


#[derive(Debug, Clone)]
pub struct DorisRequestBuilder {
    pub(super) compression: Compression,
    pub(super) encoding: (Transformer, Encoder<Framer>),
}

pub struct DorisMetadata {
    finalizers: EventFinalizers,
    batch_size: usize,
    events_byte_size: JsonSize,
    partition_key: DorisPartitionKey,
}

impl RequestBuilder<(DorisPartitionKey, Vec<Event>)> for DorisRequestBuilder {
    type Metadata = DorisMetadata;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = DorisRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoding
    }

    fn split_input(
        &self,
        input: (DorisPartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;
        let events_byte_size = events
            .iter()
            .map(|x| x.estimated_json_encoded_size_of())
            .reduce(|a, b| a + b)
            .unwrap_or(JsonSize::zero());
        info!("Event batch size: {}", events.len());
        for (i, event) in events.iter().enumerate() {
            let log = event.as_log();
            if let Ok(json) = serde_json::to_string(&log) {
                info!("Event {}: {}", i, json);
            } else {
                info!("Event {}: [Failed to serialize]", i);
            }
        }
        

        let builder = RequestMetadataBuilder::from_events(&events);
        let doris_metadata = DorisMetadata {
            finalizers: events.take_finalizers(),
            batch_size: events.len(),
            events_byte_size: events_byte_size,
            partition_key: key,
        };

        (doris_metadata, builder, events)
    }


        fn build_request(
        &self,
        doris_metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {

        DorisRequest {
            payload: payload.into_payload(),
            finalizers: doris_metadata.finalizers,
            batch_size: doris_metadata.batch_size,
            events_byte_size: doris_metadata.events_byte_size,
            metadata: request_metadata,
            partition_key: DorisPartitionKey {
                database: doris_metadata.partition_key.database,
                table: doris_metadata.partition_key.table,
            },
            redirect_url: None,
        }
    }
}


