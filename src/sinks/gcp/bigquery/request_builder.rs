use crate::codecs::encoding::ProtobufSerializer;
use bytes::BytesMut;
use prost::{Message, encoding::encoded_len_varint};
use std::num::NonZeroUsize;
use tokio_util::codec::Encoder;
use vector_lib::event::Finalizable;
use vector_lib::request_metadata::RequestMetadata;

use super::proto::google::cloud::bigquery::storage::v1 as proto;
use super::service::BigqueryRequest;
use crate::event::{Event, EventFinalizers};
use crate::sinks::util::IncrementalRequestBuilder;
use crate::sinks::util::metadata::RequestMetadataBuilder;

// 10MB maximum request size:
// https://cloud.google.com/bigquery/docs/reference/storage/rpc/google.cloud.bigquery.storage.v1#appendrowsrequest
pub const MAX_BATCH_PAYLOAD_SIZE: usize = 10_000_000;

// This buffer is subtracted from the row length budget to account for varint growth in nested message
// length prefixes. As rows fill up to 10MB, the size of the varint required to encode the length
// grows to 4 bytes (up from 1). The same growth occurs one level up, in ProtoData.
const LENGTH_PREFIX_BUFFER: usize = 6;

#[derive(Debug, snafu::Snafu)]
pub enum BigqueryRequestBuilderError {
    #[snafu(display("Encoding protobuf failed: {}", message))]
    ProtobufEncoding { message: String },
}

impl From<vector_common::Error> for BigqueryRequestBuilderError {
    fn from(error: vector_common::Error) -> Self {
        BigqueryRequestBuilderError::ProtobufEncoding {
            message: format!("{error}"),
        }
    }
}

#[derive(Default)]
pub struct BigqueryRequestMetadata {
    request_metadata: RequestMetadata,
    finalizers: EventFinalizers,
}

pub struct BigqueryRequestBuilder {
    protobuf_serializer: ProtobufSerializer,
    write_stream: String,
    // Cached schema for inclusion in every AppendRowsRequest.
    proto_schema: proto::ProtoSchema,
    // Encoded size of a full AppendRowsRequest with no rows, used to compute batch size limits.
    request_overhead: usize,
}

impl BigqueryRequestBuilder {
    pub fn new(
        protobuf_serializer: ProtobufSerializer,
        write_stream: String,
    ) -> Result<Self, BigqueryRequestBuilderError> {
        // The codecs library uses prost-reflect (prost 0.13), but the generated BigQuery proto
        // uses tonic (prost 0.12). We bridge the version gap by encoding to bytes and decoding.
        let descriptor_bytes = protobuf_serializer.encode_descriptor_proto();
        let descriptor_proto = prost_types::DescriptorProto::decode(descriptor_bytes.as_slice())
            .map_err(|e| BigqueryRequestBuilderError::ProtobufEncoding {
                message: format!("{e}"),
            })?;
        let proto_schema = proto::ProtoSchema {
            proto_descriptor: Some(descriptor_proto),
        };
        let request_overhead = proto::AppendRowsRequest {
            write_stream: write_stream.clone(),
            offset: None,
            trace_id: Default::default(),
            missing_value_interpretations: Default::default(),
            default_missing_value_interpretation: 0,
            rows: Some(proto::append_rows_request::Rows::ProtoRows(
                proto::append_rows_request::ProtoData {
                    writer_schema: Some(proto_schema.clone()),
                    rows: Some(proto::ProtoRows {
                        serialized_rows: vec![],
                    }),
                },
            )),
        }
        .encoded_len();

        if request_overhead.saturating_add(LENGTH_PREFIX_BUFFER) >= MAX_BATCH_PAYLOAD_SIZE {
            // This could only really happen if the proto schema is extremely large.
            // Unlikely to be a problem in the real world but it's better to be defensive.
            return Err(BigqueryRequestBuilderError::ProtobufEncoding {
                message: format!("Request overhead ({request_overhead} bytes) is too large"),
            });
        }

        Ok(Self {
            protobuf_serializer,
            write_stream,
            proto_schema,
            request_overhead,
        })
    }

    fn build_proto_data(
        &self,
        serialized_rows: Vec<Vec<u8>>,
    ) -> (NonZeroUsize, proto::append_rows_request::ProtoData) {
        let proto_data = proto::append_rows_request::ProtoData {
            writer_schema: Some(self.proto_schema.clone()),
            rows: Some(proto::ProtoRows { serialized_rows }),
        };
        let size = NonZeroUsize::new(proto_data.encoded_len())
            .expect("encoded payload can never be empty");
        (size, proto_data)
    }
}

impl IncrementalRequestBuilder<Vec<Event>> for BigqueryRequestBuilder {
    type Metadata = BigqueryRequestMetadata;
    type Payload = proto::append_rows_request::ProtoData;
    type Request = BigqueryRequest;
    type Error = BigqueryRequestBuilderError;

    fn encode_events_incremental(
        &mut self,
        input: Vec<Event>,
    ) -> Vec<Result<(Self::Metadata, Self::Payload), Self::Error>> {
        let max_serialized_rows_len =
            MAX_BATCH_PAYLOAD_SIZE - self.request_overhead - LENGTH_PREFIX_BUFFER;
        let mut results = vec![];
        let mut event_finalizers = EventFinalizers::DEFAULT;
        let mut chunk_metadata = RequestMetadataBuilder::default();

        let mut serialized_rows: Vec<Vec<u8>> = vec![];
        let mut serialized_rows_len = 0;
        for mut event in input {
            let mut bytes = BytesMut::new();
            if let Err(e) = self.protobuf_serializer.encode(event.clone(), &mut bytes) {
                results.push(Err(BigqueryRequestBuilderError::ProtobufEncoding {
                    message: format!("{e}"),
                }));
            } else {
                // Each row in ProtoRows.serialized_rows (field 1, wire type 2) is encoded as
                // 1 byte tag + varint(len) bytes + payload bytes.
                let row_framed_size = bytes.len() + 1 + encoded_len_varint(bytes.len() as u64);
                if row_framed_size > max_serialized_rows_len {
                    // A single event that exceeds the limit cannot be sent in any request, reject it immediately.
                    results.push(Err(BigqueryRequestBuilderError::ProtobufEncoding {
                        message: format!(
                            "Event ({row_framed_size} bytes including proto framing) exceeds the maximum allowed serialized rows size ({max_serialized_rows_len} bytes).",
                        ),
                    }));
                } else {
                    if row_framed_size + serialized_rows_len > max_serialized_rows_len {
                        // Adding this event would overflow the current chunk so flush it first.
                        let (size, proto_data) = self.build_proto_data(serialized_rows);
                        results.push(Ok((
                            BigqueryRequestMetadata {
                                finalizers: event_finalizers,
                                request_metadata: chunk_metadata.with_request_size(size),
                            },
                            proto_data,
                        )));
                        event_finalizers = EventFinalizers::DEFAULT;
                        chunk_metadata = RequestMetadataBuilder::default();
                        serialized_rows_len = 0;
                        serialized_rows = vec![];
                    }
                    let current_event_finalizers = event.take_finalizers();
                    chunk_metadata.track_event(event);
                    event_finalizers.merge(current_event_finalizers);
                    serialized_rows_len += row_framed_size;
                    serialized_rows.push(bytes.into());
                }
            }
        }
        // flush the final chunk (if there are any events left)
        if !serialized_rows.is_empty() {
            let (size, proto_data) = self.build_proto_data(serialized_rows);
            results.push(Ok((
                BigqueryRequestMetadata {
                    finalizers: event_finalizers,
                    request_metadata: chunk_metadata.with_request_size(size),
                },
                proto_data,
            )));
        }
        results
    }

    fn build_request(&mut self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let request = proto::AppendRowsRequest {
            write_stream: self.write_stream.clone(),
            offset: None, // not supported by _default stream
            trace_id: Default::default(),
            missing_value_interpretations: Default::default(),
            default_missing_value_interpretation: 0,
            rows: Some(proto::append_rows_request::Rows::ProtoRows(payload)),
        };
        let uncompressed_size = request.encoded_len();
        BigqueryRequest {
            request,
            metadata: metadata.request_metadata,
            finalizers: metadata.finalizers,
            uncompressed_size,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::codecs::encoding::{ProtobufSerializerConfig, ProtobufSerializerOptions};
    use crate::event::{Event, EventMetadata, LogEvent, Value};
    use bytes::{BufMut, Bytes, BytesMut};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::{BigqueryRequestBuilder, MAX_BATCH_PAYLOAD_SIZE};
    use crate::sinks::util::IncrementalRequestBuilder;

    const OVERSIZED_PAYLOAD: &[u8] = &[0u8; MAX_BATCH_PAYLOAD_SIZE];

    fn make_oversized_event() -> Event {
        Event::Log(LogEvent::from_parts(
            Value::Object(BTreeMap::from([(
                "binary".into(),
                Value::Bytes(Bytes::from_static(OVERSIZED_PAYLOAD)),
            )])),
            EventMetadata::default(),
        ))
    }

    fn make_request_builder() -> BigqueryRequestBuilder {
        let desc_file = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("lib/codecs/tests/data/protobuf/test.desc");
        let protobuf_serializer = ProtobufSerializerConfig {
            protobuf: ProtobufSerializerOptions {
                desc_file,
                message_type: "test.Bytes".into(),
                use_json_names: false,
            },
        }
        .build()
        .unwrap();
        BigqueryRequestBuilder::new(
            protobuf_serializer,
            "/projects/123/datasets/456/tables/789/streams/_default".to_string(),
        )
        .unwrap()
    }

    #[test]
    fn no_events() {
        let mut request_builder = make_request_builder();
        let results = request_builder.encode_events_incremental(vec![]);
        assert!(results.is_empty());
    }

    #[test]
    fn batch_is_split_into_chunks() {
        let mut request_builder = make_request_builder();
        let mut data = BytesMut::with_capacity(63336);
        for i in 1..data.capacity() {
            data.put_u64(i as u64);
        }
        let events = (0..128)
            .map(|_| {
                Event::Log(LogEvent::from_parts(
                    Value::Object(BTreeMap::from([
                        ("text".into(), Value::Bytes(Bytes::from("hello world"))),
                        ("binary".into(), Value::Bytes(data.clone().into())),
                    ])),
                    EventMetadata::default(),
                ))
            })
            .collect();
        let results = request_builder.encode_events_incremental(events);
        assert!(results.iter().all(|r| r.is_ok()));
        assert!(results.len() > 1);
    }

    #[test]
    fn oversized_event_is_rejected() {
        let mut request_builder = make_request_builder();
        let results = request_builder.encode_events_incremental(vec![make_oversized_event()]);
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
    }

    #[test]
    fn oversized_event_mixed_with_normal_event() {
        let mut request_builder = make_request_builder();
        let normal_event = Event::Log(LogEvent::from_parts(
            Value::Object(BTreeMap::from([(
                "text".into(),
                Value::Bytes(Bytes::from("hello")),
            )])),
            EventMetadata::default(),
        ));
        let results =
            request_builder.encode_events_incremental(vec![normal_event, make_oversized_event()]);
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1);
    }
}
