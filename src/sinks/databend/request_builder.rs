use std::io;

use bytes::Bytes;
use chrono::Utc;
use serde_json::Value as JsonValue;
use uuid::Uuid;
use vector_lib::{
    codecs::encoding::Framer,
    event::{Event, LogEvent, ObjectMap, Value},
    finalization::{EventFinalizers, Finalizable},
    request_metadata::RequestMetadata,
};

use super::{config::DatabendRawOptions, service::DatabendRequest};
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

    fn raw_i64(log: &LogEvent, path: &str) -> Option<i64> {
        Self::get_path(log, path).and_then(Value::as_integer)
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

    fn insert_metadata_path(record_metadata: &mut ObjectMap, path: &str, value: Value) {
        let path = path.trim_start_matches('%');
        let mut segments = path.split('.').filter(|segment| !segment.is_empty());
        let Some(first) = segments.next() else {
            return;
        };

        let mut current = record_metadata
            .entry(first.into())
            .or_insert_with(|| Value::Object(ObjectMap::new()));

        for segment in segments {
            if !matches!(current, Value::Object(_)) {
                *current = Value::Object(ObjectMap::new());
            }
            let Value::Object(map) = current else {
                return;
            };
            current = map
                .entry(segment.into())
                .or_insert_with(|| Value::Object(ObjectMap::new()));
        }

        *current = value;
    }

    fn record_metadata(&self, log: &LogEvent) -> ObjectMap {
        let mut record_metadata = ObjectMap::new();

        for path in &self.raw.metadata.includes {
            if path == "*" {
                if let Value::Object(metadata) = log.metadata().value() {
                    record_metadata.extend(metadata.clone());
                }
                continue;
            }

            if let Some(value) = Self::get_path(log, path) {
                Self::insert_metadata_path(&mut record_metadata, path, value.clone());
            }
        }

        record_metadata
    }

    fn raw_record(&self, event: Event) -> Event {
        let Event::Log(log) = &event else {
            return event;
        };

        let offset = Self::raw_i64(log, "%kafka.offset").unwrap_or_default();
        let partition = Self::raw_i64(log, "%kafka.partition").unwrap_or_default();
        let raw_data = Self::raw_payload(log, &self.raw.message_key);
        let record_metadata = self.record_metadata(log);

        let mut raw_log = LogEvent::default();
        raw_log.insert("uuid", Uuid::new_v4().to_string());
        raw_log.insert("koffset", offset);
        raw_log.insert("kpartition", partition);
        raw_log.insert("raw_data", raw_data);
        raw_log.insert("record_metadata", Value::Object(record_metadata));
        raw_log.insert("add_time", Utc::now());
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
