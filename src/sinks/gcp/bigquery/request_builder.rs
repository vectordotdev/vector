use bytes::BytesMut;
use codecs::encoding::ProtobufSerializer;
use prost::Message;
use std::num::NonZeroUsize;
use tokio_util::codec::Encoder;
use vector_common::request_metadata::RequestMetadata;
use vector_core::event::Finalizable;

use super::proto::google::cloud::bigquery::storage::v1 as proto;
use super::service::BigqueryRequest;
use crate::event::{Event, EventFinalizers};
use crate::sinks::util::metadata::RequestMetadataBuilder;
use crate::sinks::util::IncrementalRequestBuilder;

// 10MB maximum message size:
// https://cloud.google.com/bigquery/docs/reference/storage/rpc/google.cloud.bigquery.storage.v1#appendrowsrequest
pub const MAX_BATCH_PAYLOAD_SIZE: usize = 10_000_000;

#[derive(Debug, snafu::Snafu)]
pub enum BigqueryRequestBuilderError {
    #[snafu(display("Encoding protobuf failed: {}", message))]
    ProtobufEncoding { message: String }, // `error` needs to be some concrete type
}

impl From<vector_common::Error> for BigqueryRequestBuilderError {
    fn from(error: vector_common::Error) -> Self {
        BigqueryRequestBuilderError::ProtobufEncoding {
            message: format!("{:?}", error),
        }
    }
}

#[derive(Default)]
pub struct BigqueryRequestMetadata {
    request_metadata: RequestMetadata,
    finalizers: EventFinalizers,
}

pub struct BigqueryRequestBuilder {
    pub protobuf_serializer: ProtobufSerializer,
    pub write_stream: String,
}

impl BigqueryRequestBuilder {
    fn build_proto_data(
        &self,
        serialized_rows: Vec<Vec<u8>>,
    ) -> (NonZeroUsize, proto::append_rows_request::ProtoData) {
        let proto_data = proto::append_rows_request::ProtoData {
            writer_schema: Some(proto::ProtoSchema {
                proto_descriptor: Some(self.protobuf_serializer.descriptor_proto().clone()),
            }),
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
        let base_proto_data_size: NonZeroUsize = self.build_proto_data(vec![]).0;
        let max_serialized_rows_len: usize = MAX_BATCH_PAYLOAD_SIZE - base_proto_data_size.get();
        let metadata = RequestMetadataBuilder::from_events(&input);
        let mut errors: Vec<Self::Error> = vec![];
        let mut bodies: Vec<(EventFinalizers, (NonZeroUsize, Self::Payload))> = vec![];
        let mut event_finalizers = EventFinalizers::DEFAULT;
        let mut serialized_rows: Vec<Vec<u8>> = vec![];
        let mut serialized_rows_len: usize = 0;
        for mut event in input.into_iter() {
            let current_event_finalizers = event.take_finalizers();
            let mut bytes = BytesMut::new();
            if let Err(e) = self.protobuf_serializer.encode(event, &mut bytes) {
                errors.push(BigqueryRequestBuilderError::ProtobufEncoding {
                    message: format!("{:?}", e),
                });
            } else {
                if bytes.len() + serialized_rows_len > max_serialized_rows_len {
                    // there's going to be too many events to send in one body;
                    // flush the current events and start a new body
                    bodies.push((event_finalizers, self.build_proto_data(serialized_rows)));
                    event_finalizers = EventFinalizers::DEFAULT;
                    serialized_rows = vec![];
                    serialized_rows_len = 0;
                }
                event_finalizers.merge(current_event_finalizers);
                serialized_rows_len += bytes.len();
                serialized_rows.push(bytes.into());
            }
        }
        // flush the final body (if there are any events left)
        if !serialized_rows.is_empty() {
            bodies.push((event_finalizers, self.build_proto_data(serialized_rows)));
        }
        // throw everything together into the expected IncrementalRequestBuilder return type
        bodies
            .into_iter()
            .map(|(event_finalizers, (size, proto_data))| {
                Ok((
                    BigqueryRequestMetadata {
                        finalizers: event_finalizers,
                        request_metadata: metadata.with_request_size(size),
                    },
                    proto_data,
                ))
            })
            .chain(errors.into_iter().map(Err))
            .collect()
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
    use bytes::{BufMut, Bytes, BytesMut};
    use codecs::encoding::{ProtobufSerializerConfig, ProtobufSerializerOptions};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use vector_core::event::{Event, EventMetadata, LogEvent, Value};

    use super::BigqueryRequestBuilder;
    use crate::sinks::util::IncrementalRequestBuilder;

    #[test]
    fn encode_events_incremental() {
        // build the request builder
        let desc_file = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("lib/codecs/tests/data/protobuf/test.desc");
        let protobuf_serializer = ProtobufSerializerConfig {
            protobuf: ProtobufSerializerOptions {
                desc_file,
                message_type: "test.Bytes".into(),
            },
        }
        .build()
        .unwrap();
        let mut request_builder = BigqueryRequestBuilder {
            protobuf_serializer,
            write_stream: "/projects/123/datasets/456/tables/789/streams/_default".to_string(),
        };
        // check that we break up large batches to avoid api limits
        let mut events = vec![];
        let mut data = BytesMut::with_capacity(63336);
        for i in 1..data.capacity() {
            data.put_u64(i as u64);
        }
        for _ in 0..128 {
            let event = Event::Log(LogEvent::from_parts(
                Value::Object(BTreeMap::from([
                    ("text".into(), Value::Bytes(Bytes::from("hello world"))),
                    ("binary".into(), Value::Bytes(data.clone().into())),
                ])),
                EventMetadata::default(),
            ));
            events.push(event);
        }
        let results = request_builder.encode_events_incremental(events);
        assert!(results.iter().all(|r| r.is_ok()));
        assert!(results.len() > 1);
        // check that we don't generate bodies with no events in them
        let results = request_builder.encode_events_incremental(vec![]);
        assert!(results.is_empty());
    }
}
