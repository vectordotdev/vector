//! Protobuf batch serializer for encoding events as individual protobuf records.
//!
//! Encodes each event in a batch independently into protobuf bytes, producing
//! a `Vec<Vec<u8>>` where each element is a single serialized protobuf message.

use prost_reflect::{MessageDescriptor, prost::Message as _};
use snafu::Snafu;
use std::sync::Arc;
use vector_config::configurable_component;
use vector_core::{config::DataType, event::Event, schema};
use vrl::protobuf::encode::{Options, encode_message};

/// Errors that can occur during protobuf batch encoding
#[derive(Debug, Snafu)]
pub enum ProtoBatchEncodingError {
    /// No events provided
    #[snafu(display("Cannot encode an empty batch"))]
    NoEvents,

    /// Unsupported event type
    #[snafu(display("Unsupported event type: only Log events are supported"))]
    UnsupportedEventType,

    /// Protobuf encoding failed
    #[snafu(display("Protobuf encoding failed: {}", source))]
    EncodingFailed {
        /// The underlying encoding error
        source: vector_common::Error,
    },

    /// Protobuf prost encoding failed
    #[snafu(display("Protobuf prost encoding failed: {}", source))]
    ProstEncodingFailed {
        /// The underlying prost error
        source: prost_reflect::prost::EncodeError,
    },
}

/// Configuration for protobuf batch serialization
#[configurable_component]
#[derive(Clone, Default)]
pub struct ProtoBatchSerializerConfig {
    /// The protobuf message descriptor to use for encoding.
    #[serde(skip)]
    #[configurable(derived)]
    pub descriptor: Option<MessageDescriptor>,
}

impl std::fmt::Debug for ProtoBatchSerializerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtoBatchSerializerConfig")
            .field(
                "descriptor",
                &self.descriptor.as_ref().map(|d| d.full_name().to_string()),
            )
            .finish()
    }
}

impl ProtoBatchSerializerConfig {
    /// Create a new ProtoBatchSerializerConfig with a message descriptor
    pub fn new(descriptor: MessageDescriptor) -> Self {
        Self {
            descriptor: Some(descriptor),
        }
    }

    /// The data type of events that are accepted by this serializer.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        schema::Requirement::empty()
    }
}

/// Protobuf batch serializer that encodes each event into individual protobuf bytes.
#[derive(Clone, Debug)]
pub struct ProtoBatchSerializer {
    descriptor: Arc<MessageDescriptor>,
    options: Options,
}

impl ProtoBatchSerializer {
    /// Create a new ProtoBatchSerializer with the given configuration.
    pub fn new(config: ProtoBatchSerializerConfig) -> Result<Self, vector_common::Error> {
        let descriptor = config.descriptor.ok_or_else(|| {
            vector_common::Error::from("Proto batch serializer requires a message descriptor.")
        })?;

        Ok(Self {
            descriptor: Arc::new(descriptor),
            options: Options {
                use_json_names: false,
            },
        })
    }

    /// Encode a batch of events into individual protobuf byte buffers.
    pub fn encode_batch(&self, events: &[Event]) -> Result<Vec<Vec<u8>>, ProtoBatchEncodingError> {
        if events.is_empty() {
            return Err(ProtoBatchEncodingError::NoEvents);
        }

        let mut records = Vec::with_capacity(events.len());

        for event in events {
            let dynamic_message = match event {
                Event::Log(log) => {
                    encode_message(&self.descriptor, log.value().clone(), &self.options)
                }
                Event::Trace(_) | Event::Metric(_) => {
                    return Err(ProtoBatchEncodingError::UnsupportedEventType);
                }
            }
            .map_err(|source| ProtoBatchEncodingError::EncodingFailed {
                source: source.into(),
            })?;

            records.push(dynamic_message.encode_to_vec());
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_reflect::{
        DescriptorPool, DynamicMessage, Value as ProstValue,
        prost_types::{
            DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet,
            field_descriptor_proto::{Label, Type},
        },
    };
    use vector_core::event::{LogEvent, Metric, MetricKind, MetricValue, TraceEvent, Value};
    use vrl::btreemap;

    fn build_descriptor() -> MessageDescriptor {
        // message Inner { string label = 1; }
        let inner = DescriptorProto {
            name: Some("Inner".to_string()),
            field: vec![FieldDescriptorProto {
                name: Some("label".to_string()),
                number: Some(1),
                label: Some(Label::Optional as i32),
                r#type: Some(Type::String as i32),
                ..Default::default()
            }],
            ..Default::default()
        };

        // message Outer { string name = 1; int64 count = 2; Inner inner = 3; }
        let outer = DescriptorProto {
            name: Some("Outer".to_string()),
            field: vec![
                FieldDescriptorProto {
                    name: Some("name".to_string()),
                    number: Some(1),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::String as i32),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("count".to_string()),
                    number: Some(2),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::Int64 as i32),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("inner".to_string()),
                    number: Some(3),
                    label: Some(Label::Optional as i32),
                    r#type: Some(Type::Message as i32),
                    type_name: Some(".test.Inner".to_string()),
                    ..Default::default()
                },
            ],
            nested_type: vec![],
            ..Default::default()
        };

        let file = FileDescriptorProto {
            name: Some("test.proto".to_string()),
            package: Some("test".to_string()),
            message_type: vec![outer, inner],
            syntax: Some("proto3".to_string()),
            ..Default::default()
        };

        let pool = DescriptorPool::from_file_descriptor_set(FileDescriptorSet { file: vec![file] })
            .expect("descriptor pool builds");
        pool.get_message_by_name("test.Outer")
            .expect("Outer message exists")
    }

    fn make_serializer() -> ProtoBatchSerializer {
        ProtoBatchSerializer::new(ProtoBatchSerializerConfig::new(build_descriptor()))
            .expect("serializer builds")
    }

    #[test]
    fn empty_batch_returns_no_events_error() {
        let serializer = make_serializer();
        let err = serializer
            .encode_batch(&[])
            .expect_err("empty batch errors");
        assert!(matches!(err, ProtoBatchEncodingError::NoEvents));
    }

    #[test]
    fn metric_event_is_rejected() {
        let serializer = make_serializer();
        let metric = Event::Metric(Metric::new(
            "test",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        ));
        let err = serializer
            .encode_batch(&[metric])
            .expect_err("metric event errors");
        assert!(matches!(err, ProtoBatchEncodingError::UnsupportedEventType));
    }

    #[test]
    fn trace_event_is_rejected() {
        let serializer = make_serializer();
        let trace = Event::Trace(TraceEvent::default());
        let err = serializer
            .encode_batch(&[trace])
            .expect_err("trace event errors");
        assert!(matches!(err, ProtoBatchEncodingError::UnsupportedEventType));
    }

    #[test]
    fn round_trip_decode_preserves_field_mapping() {
        let descriptor = build_descriptor();
        let serializer =
            ProtoBatchSerializer::new(ProtoBatchSerializerConfig::new(descriptor.clone()))
                .expect("serializer builds");

        let event = Event::Log(LogEvent::from(btreemap! {
            "name" => Value::from("hello"),
            "count" => Value::from(42_i64),
            "inner" => Value::from(btreemap! {
                "label" => Value::from("nested"),
            }),
        }));

        let records = serializer
            .encode_batch(&[event])
            .expect("encoding succeeds");
        assert_eq!(records.len(), 1);

        let decoded =
            DynamicMessage::decode(descriptor, records[0].as_slice()).expect("decode succeeds");

        let name_field = decoded
            .get_field_by_name("name")
            .expect("name field present");
        assert_eq!(name_field.as_str(), Some("hello"));

        let count_field = decoded
            .get_field_by_name("count")
            .expect("count field present");
        assert_eq!(count_field.as_i64(), Some(42));

        let inner_field = decoded
            .get_field_by_name("inner")
            .expect("inner field present");
        let inner_msg = match &*inner_field {
            ProstValue::Message(m) => m,
            other => panic!("expected nested message, got {:?}", other),
        };
        let label = inner_msg
            .get_field_by_name("label")
            .expect("label field present");
        assert_eq!(label.as_str(), Some("nested"));
    }
}
