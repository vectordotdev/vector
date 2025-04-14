//! `RequestBuilder` implementation for the `Doris` sink.

use super::sink::DorisPartitionKey;
use crate::sinks::doris::service::DorisRequest;
use crate::sinks::prelude::*;
use bytes::Bytes;
use vector_lib::codecs::encoding::Framer;

#[derive(Debug, Clone)]
pub struct DorisRequestBuilder {
    pub(super) compression: Compression,
    pub(super) encoder: (Transformer, Encoder<Framer>),
}

pub struct DorisMetadata {
    finalizers: EventFinalizers,
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
        &self.encoder
    }

    fn split_input(
        &self,
        input: (DorisPartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;

        let builder = RequestMetadataBuilder::from_events(&events);
        let doris_metadata = DorisMetadata {
            finalizers: events.take_finalizers(),
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
            metadata: request_metadata,
            partition_key: DorisPartitionKey {
                database: doris_metadata.partition_key.database,
                table: doris_metadata.partition_key.table,
            },
            redirect_url: None,
        }
    }
}
