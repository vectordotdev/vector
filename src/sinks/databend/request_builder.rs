use std::io;

use bytes::Bytes;
use chrono::Utc;
use serde_json::Value as JsonValue;
use vector_lib::{
    codecs::encoding::Framer,
    event::{Event, LogEvent, Value},
    finalization::{EventFinalizers, Finalizable},
    lookup::{OwnedTargetPath, OwnedValuePath},
    request_metadata::RequestMetadata,
};

use super::{
    config::{DatabendRawOptions, raw_metadata_column_name},
    service::DatabendRequest,
};
use crate::{
    codecs::{Encoder, Transformer},
    sinks::util::{
        Compression, RequestBuilder, encoding::Encoder as RequestEncoder,
        metadata::RequestMetadataBuilder, request_builder::EncodeResult,
    },
};

#[derive(Clone)]
pub struct DatabendRequestBuilder {
    compression: Compression,
    encoder: (Transformer, Encoder<Framer>),
    raw: DatabendRawOptions,
}

impl DatabendRequestBuilder {
    pub const fn new(
        compression: Compression,
        encoder: (Transformer, Encoder<Framer>),
        raw: DatabendRawOptions,
    ) -> Self {
        Self {
            compression,
            encoder,
            raw,
        }
    }

    fn get_path<'a>(log: &'a LogEvent, path: &str) -> Option<&'a Value> {
        log.parse_path_and_get_value(path).ok().flatten()
    }

    fn raw_payload(log: &LogEvent, path: &str) -> Value {
        let Some(value) = Self::get_path(log, path) else {
            return Value::Null;
        };

        match value {
            Value::Bytes(bytes) => serde_json::from_slice::<JsonValue>(bytes)
                .ok()
                .map(Value::from)
                .unwrap_or_else(|| Value::Bytes(bytes.clone())),
            value => value.clone(),
        }
    }

    fn insert_raw_column(raw_log: &mut LogEvent, column: &str, value: Value) {
        let path = OwnedTargetPath::event(OwnedValuePath::single_field(column));
        raw_log.insert(&path, value);
    }

    fn raw_record(&self, event: Event) -> Event {
        let Event::Log(log) = &event else {
            return event;
        };

        let raw_data = Self::raw_payload(log, &self.raw.message_key);

        let mut raw_log = LogEvent::default();
        raw_log.insert("raw_data", raw_data);
        raw_log.insert("add_time", Utc::now());

        for path in &self.raw.metadata.includes {
            let Some(column) = raw_metadata_column_name(path) else {
                continue;
            };

            if path == "*" {
                Self::insert_raw_column(&mut raw_log, &column, log.metadata().value().clone());
                continue;
            }

            if let Some(value) = Self::get_path(log, path) {
                Self::insert_raw_column(&mut raw_log, &column, value.clone());
            }
        }

        Event::Log(raw_log)
    }
}

impl RequestBuilder<Vec<Event>> for DatabendRequestBuilder {
    type Metadata = EventFinalizers;
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = DatabendRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn encode_events(
        &self,
        events: Self::Events,
    ) -> Result<EncodeResult<Self::Payload>, Self::Error> {
        let events = if self.raw.enabled {
            events
                .into_iter()
                .map(|event| self.raw_record(event))
                .collect::<Vec<_>>()
        } else {
            events
        };

        let mut compressor = crate::sinks::util::Compressor::from(self.compression());
        let is_compressed = compressor.is_compressed();
        let (_, json_size) = self.encoder().encode_input(events, &mut compressor)?;

        let payload = compressor.into_inner().freeze();
        let result = if is_compressed {
            let compressed_byte_size = payload.len();
            EncodeResult::compressed(payload, compressed_byte_size, json_size)
        } else {
            EncodeResult::uncompressed(payload, json_size)
        };

        Ok(result)
    }

    fn split_input(
        &self,
        input: Vec<Event>,
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let mut events = input;
        let finalizers = events.take_finalizers();
        let builder = RequestMetadataBuilder::from_events(&events);
        (finalizers, builder, events)
    }

    fn build_request(
        &self,
        finalizers: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        DatabendRequest {
            finalizers,
            data: payload.into_payload(),
            metadata,
        }
    }
}
