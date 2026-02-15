//! `RequestBuilder` implementation for the `Clickhouse` sink.

use bytes::Bytes;
use std::io;

use super::sink::PartitionKey;
use crate::sinks::prelude::*;
use crate::sinks::util::http::HttpRequest;
use vector_lib::codecs::EncoderKind;
#[cfg(feature = "codecs-arrow")]
use vector_lib::codecs::internal_events::EncoderNullConstraintError;
use vector_lib::lookup;

pub(super) struct ClickhouseRequestBuilder {
    pub(super) compression: Compression,
    pub(super) encoder: (Transformer, EncoderKind),
    pub(super) required_fields: Option<Vec<String>>,
}

impl RequestBuilder<(PartitionKey, Vec<Event>)> for ClickhouseRequestBuilder {
    type Metadata = (PartitionKey, EventFinalizers);
    type Events = Vec<Event>;
    type Encoder = (Transformer, EncoderKind);
    type Payload = Bytes;
    type Request = HttpRequest<PartitionKey>;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn pre_encode(&self, events: &Self::Events) -> Result<(), Self::Error> {
        self.validate_required_fields(events)
    }

    fn split_input(
        &self,
        input: (PartitionKey, Vec<Event>),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;

        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        ((key, finalizers), builder, events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (key, finalizers) = metadata;
        HttpRequest::new(
            payload.into_payload(),
            finalizers,
            request_metadata,
            PartitionKey {
                database: key.database,
                table: key.table,
                format: key.format,
            },
        )
    }
}

impl ClickhouseRequestBuilder {
    fn validate_required_fields(&self, events: &[Event]) -> Result<(), io::Error> {
        let Some(required_fields) = &self.required_fields else {
            return Ok(());
        };

        if required_fields.is_empty() {
            return Ok(());
        }

        for event in events.iter().filter_map(Event::maybe_as_log) {
            for field in required_fields {
                if event.get(lookup::event_path!(field)).is_none() {
                    #[cfg(feature = "codecs-arrow")]
                    {
                        let error: vector_common::Error = Box::new(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Missing required field '{field}'"),
                        ));
                        emit!(EncoderNullConstraintError { error: &error });
                    }
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Missing required field '{field}'"),
                    ));
                }
            }
        }

        Ok(())
    }
}
