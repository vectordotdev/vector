use crate::common::protobuf::get_message_descriptor;
use crate::encoding::BuildError;
use bytes::BytesMut;
use chrono::Timelike;
use prost::Message;
use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MapKey, MessageDescriptor};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio_util::codec::Encoder;
use vector_core::{
    config::DataType,
    event::{Event, Value},
    schema,
};

/// Config used to build a `ProtobufSerializer`.
#[crate::configurable_component]
#[derive(Debug, Clone)]
pub struct ProtobufSerializerConfig {
    /// Options for the Protobuf serializer.
    pub protobuf: ProtobufSerializerOptions,
}

impl ProtobufSerializerConfig {
    /// Build the `ProtobufSerializer` from this configuration.
    pub fn build(&self) -> Result<ProtobufSerializer, BuildError> {
        let message_descriptor =
            get_message_descriptor(&self.protobuf.desc_file, &self.protobuf.message_type)?;
        Ok(ProtobufSerializer { message_descriptor })
    }

    /// The data type of events that are accepted by `ProtobufSerializer`.
    pub fn input_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema required by the serializer.
    pub fn schema_requirement(&self) -> schema::Requirement {
        // While technically we support `Value` variants that can't be losslessly serialized to
        // Protobuf, we don't want to enforce that limitation to users yet.
        schema::Requirement::empty()
    }
}

/// Protobuf serializer options.
#[crate::configurable_component]
#[derive(Debug, Clone)]
pub struct ProtobufSerializerOptions {
    /// The path to the protobuf descriptor set file.
    ///
    /// This file is the output of `protoc -o <path> ...`
    #[configurable(metadata(docs::examples = "/etc/vector/protobuf_descriptor_set.desc"))]
    pub desc_file: PathBuf,

    /// The name of the message type to use for serializing.
    #[configurable(metadata(docs::examples = "package.Message"))]
    pub message_type: String,
}

/// Serializer that converts an `Event` to bytes using the Protobuf format.
#[derive(Debug, Clone)]
pub struct ProtobufSerializer {
    /// The protobuf message definition to use for serialization.
    message_descriptor: MessageDescriptor,
}

/// Convert a single raw vector `Value` into a protobuf `Value`.
///
/// Unlike `convert_value`, this ignores any field metadata such as cardinality.
fn convert_value_raw(
    value: Value,
    kind: &prost_reflect::Kind,
) -> Result<prost_reflect::Value, vector_common::Error> {
    let kind_str = value.kind_str().to_owned();
    match (value, kind) {
        (Value::Boolean(b), Kind::Bool) => Ok(prost_reflect::Value::Bool(b)),
        (Value::Bytes(b), Kind::Bytes) => Ok(prost_reflect::Value::Bytes(b)),
        (Value::Bytes(b), Kind::String) => Ok(prost_reflect::Value::String(
            String::from_utf8_lossy(&b).into_owned(),
        )),
        (Value::Bytes(b), Kind::Enum(descriptor)) => {
            let string = String::from_utf8_lossy(&b).into_owned();
            if let Some(d) = descriptor
                .values()
                .find(|v| v.name().eq_ignore_ascii_case(&string))
            {
                Ok(prost_reflect::Value::EnumNumber(d.number()))
            } else {
                Err(format!(
                    "Enum `{}` has no value that matches string '{}'",
                    descriptor.full_name(),
                    string
                )
                .into())
            }
        }
        (Value::Float(f), Kind::Double) => Ok(prost_reflect::Value::F64(f.into_inner())),
        (Value::Float(f), Kind::Float) => Ok(prost_reflect::Value::F32(f.into_inner() as f32)),
        (Value::Integer(i), Kind::Int32) => Ok(prost_reflect::Value::I32(i as i32)),
        (Value::Integer(i), Kind::Int64) => Ok(prost_reflect::Value::I64(i)),
        (Value::Integer(i), Kind::Sint32) => Ok(prost_reflect::Value::I32(i as i32)),
        (Value::Integer(i), Kind::Sint64) => Ok(prost_reflect::Value::I64(i)),
        (Value::Integer(i), Kind::Sfixed32) => Ok(prost_reflect::Value::I32(i as i32)),
        (Value::Integer(i), Kind::Sfixed64) => Ok(prost_reflect::Value::I64(i)),
        (Value::Integer(i), Kind::Uint32) => Ok(prost_reflect::Value::U32(i as u32)),
        (Value::Integer(i), Kind::Uint64) => Ok(prost_reflect::Value::U64(i as u64)),
        (Value::Integer(i), Kind::Fixed32) => Ok(prost_reflect::Value::U32(i as u32)),
        (Value::Integer(i), Kind::Fixed64) => Ok(prost_reflect::Value::U64(i as u64)),
        (Value::Integer(i), Kind::Enum(_)) => Ok(prost_reflect::Value::EnumNumber(i as i32)),
        (Value::Object(o), Kind::Message(message_descriptor)) => {
            if message_descriptor.is_map_entry() {
                let value_field = message_descriptor
                    .get_field_by_name("value")
                    .ok_or("Internal error with proto map processing")?;
                let mut map: HashMap<MapKey, prost_reflect::Value> = HashMap::new();
                for (key, val) in o.into_iter() {
                    match convert_value(&value_field, val) {
                        Ok(prost_val) => {
                            map.insert(MapKey::String(key.into()), prost_val);
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(prost_reflect::Value::Map(map))
            } else {
                // if it's not a map, it's an actual message
                Ok(prost_reflect::Value::Message(encode_message(
                    message_descriptor,
                    Value::Object(o),
                )?))
            }
        }
        (Value::Regex(r), Kind::String) => Ok(prost_reflect::Value::String(r.as_str().to_owned())),
        (Value::Regex(r), Kind::Bytes) => Ok(prost_reflect::Value::Bytes(r.as_bytes())),
        (Value::Timestamp(t), Kind::Int64) => Ok(prost_reflect::Value::I64(t.timestamp_micros())),
        (Value::Timestamp(t), Kind::Message(descriptor))
            if descriptor.full_name() == "google.protobuf.Timestamp" =>
        {
            let mut message = DynamicMessage::new(descriptor.clone());
            message.try_set_field_by_name("seconds", prost_reflect::Value::I64(t.timestamp()))?;
            message
                .try_set_field_by_name("nanos", prost_reflect::Value::I32(t.nanosecond() as i32))?;
            Ok(prost_reflect::Value::Message(message))
        }
        _ => Err(format!("Cannot encode vector `{kind_str}` into protobuf `{kind:?}`",).into()),
    }
}

/// Convert a vector `Value` into a protobuf `Value`.
fn convert_value(
    field_descriptor: &FieldDescriptor,
    value: Value,
) -> Result<prost_reflect::Value, vector_common::Error> {
    if let Value::Array(a) = value {
        if field_descriptor.cardinality() == prost_reflect::Cardinality::Repeated {
            let repeated: Result<Vec<prost_reflect::Value>, vector_common::Error> = a
                .into_iter()
                .map(|v| convert_value_raw(v, &field_descriptor.kind()))
                .collect();
            Ok(prost_reflect::Value::List(repeated?))
        } else {
            Err("Cannot encode vector array into a non-repeated protobuf field".into())
        }
    } else {
        convert_value_raw(value, &field_descriptor.kind())
    }
}

/// Convert a vector object (`Value`) into a protobuf message.
///
/// This function can only operate on `Value::Object`s,
/// since they are the only field-based vector Value
/// and protobuf messages are defined as a collection of fields and values.
fn encode_message(
    message_descriptor: &MessageDescriptor,
    value: Value,
) -> Result<DynamicMessage, vector_common::Error> {
    let mut message = DynamicMessage::new(message_descriptor.clone());
    if let Value::Object(map) = value {
        for field in message_descriptor.fields() {
            match map.get(field.name()) {
                None | Some(Value::Null) => message.clear_field(&field),
                Some(value) => {
                    message.try_set_field(&field, convert_value(&field, value.clone())?)?
                }
            }
        }
        Ok(message)
    } else {
        Err("ProtobufSerializer only supports serializing objects".into())
    }
}

impl ProtobufSerializer {
    /// Creates a new `ProtobufSerializer`.
    pub fn new(message_descriptor: MessageDescriptor) -> Self {
        Self { message_descriptor }
    }

    /// Get a description of the message type used in serialization.
    pub fn descriptor_proto(&self) -> &prost_reflect::prost_types::DescriptorProto {
        self.message_descriptor.descriptor_proto()
    }
}

impl Encoder<Event> for ProtobufSerializer {
    type Error = vector_common::Error;

    fn encode(&mut self, event: Event, buffer: &mut BytesMut) -> Result<(), Self::Error> {
        let message = match event {
            Event::Log(log) => encode_message(&self.message_descriptor, log.into_parts().0),
            Event::Metric(_) => unimplemented!(),
            Event::Trace(trace) => encode_message(
                &self.message_descriptor,
                Value::Object(trace.into_parts().0),
            ),
        }?;
        message.encode(buffer).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use chrono::{DateTime, NaiveDateTime, Utc};
    use ordered_float::NotNan;
    use prost_reflect::MapKey;
    use std::collections::{BTreeMap, HashMap};

    macro_rules! mfield {
        ($m:expr, $f:expr) => {
            $m.get_field_by_name($f).unwrap().into_owned()
        };
    }

    fn test_data_dir() -> PathBuf {
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("tests/data/protobuf")
    }

    fn test_message_descriptor(message_type: &str) -> MessageDescriptor {
        let path = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("tests/data/protobuf/protos/test.desc");
        get_message_descriptor(&path, &format!("test.{message_type}")).unwrap()
    }

    #[test]
    fn test_config_input_type() {
        let config = ProtobufSerializerConfig {
            protobuf: ProtobufSerializerOptions {
                desc_file: test_data_dir().join("test_protobuf.desc"),
                message_type: "test_protobuf.Person".into(),
            },
        };
        assert_eq!(config.input_type(), DataType::Log);
    }

    #[test]
    fn test_encode_integers() {
        let message = encode_message(
            &test_message_descriptor("Integers"),
            Value::Object(BTreeMap::from([
                ("i32".into(), Value::Integer(-1234)),
                ("i64".into(), Value::Integer(-9876)),
                ("u32".into(), Value::Integer(1234)),
                ("u64".into(), Value::Integer(9876)),
            ])),
        )
        .unwrap();
        assert_eq!(Some(-1234), mfield!(message, "i32").as_i32());
        assert_eq!(Some(-9876), mfield!(message, "i64").as_i64());
        assert_eq!(Some(1234), mfield!(message, "u32").as_u32());
        assert_eq!(Some(9876), mfield!(message, "u64").as_u64());
    }

    #[test]
    fn test_encode_floats() {
        let message = encode_message(
            &test_message_descriptor("Floats"),
            Value::Object(BTreeMap::from([
                ("d".into(), Value::Float(NotNan::new(11.0).unwrap())),
                ("f".into(), Value::Float(NotNan::new(2.0).unwrap())),
            ])),
        )
        .unwrap();
        assert_eq!(Some(11.0), mfield!(message, "d").as_f64());
        assert_eq!(Some(2.0), mfield!(message, "f").as_f32());
    }

    #[test]
    fn test_encode_bytes() {
        let bytes = Bytes::from(vec![0, 1, 2, 3]);
        let message = encode_message(
            &test_message_descriptor("Bytes"),
            Value::Object(BTreeMap::from([
                ("text".into(), Value::Bytes(Bytes::from("vector"))),
                ("binary".into(), Value::Bytes(bytes.clone())),
            ])),
        )
        .unwrap();
        assert_eq!(Some("vector"), mfield!(message, "text").as_str());
        assert_eq!(Some(&bytes), mfield!(message, "binary").as_bytes());
    }

    #[test]
    fn test_encode_map() {
        let message = encode_message(
            &test_message_descriptor("Map"),
            Value::Object(BTreeMap::from([
                (
                    "names".into(),
                    Value::Object(BTreeMap::from([
                        ("forty-four".into(), Value::Integer(44)),
                        ("one".into(), Value::Integer(1)),
                    ])),
                ),
                (
                    "people".into(),
                    Value::Object(BTreeMap::from([(
                        "mark".into(),
                        Value::Object(BTreeMap::from([
                            ("nickname".into(), Value::Bytes(Bytes::from("jeff"))),
                            ("age".into(), Value::Integer(22)),
                        ])),
                    )])),
                ),
            ])),
        )
        .unwrap();
        // the simpler string->primitive map
        assert_eq!(
            Some(&HashMap::from([
                (
                    MapKey::String("forty-four".into()),
                    prost_reflect::Value::I32(44),
                ),
                (MapKey::String("one".into()), prost_reflect::Value::I32(1),),
            ])),
            mfield!(message, "names").as_map()
        );
        // the not-simpler string->message map
        let people = mfield!(message, "people").as_map().unwrap().to_owned();
        assert_eq!(1, people.len());
        assert_eq!(
            Some("jeff"),
            mfield!(
                people[&MapKey::String("mark".into())].as_message().unwrap(),
                "nickname"
            )
            .as_str()
        );
        assert_eq!(
            Some(22),
            mfield!(
                people[&MapKey::String("mark".into())].as_message().unwrap(),
                "age"
            )
            .as_u32()
        );
    }

    #[test]
    fn test_encode_enum() {
        let message = encode_message(
            &test_message_descriptor("Enum"),
            Value::Object(BTreeMap::from([
                ("breakfast".into(), Value::Bytes(Bytes::from("tomato"))),
                ("dinner".into(), Value::Bytes(Bytes::from("OLIVE"))),
                ("lunch".into(), Value::Integer(0)),
            ])),
        )
        .unwrap();
        assert_eq!(Some(2), mfield!(message, "breakfast").as_enum_number());
        assert_eq!(Some(0), mfield!(message, "lunch").as_enum_number());
        assert_eq!(Some(1), mfield!(message, "dinner").as_enum_number());
    }

    #[test]
    fn test_encode_timestamp() {
        let message = encode_message(
            &test_message_descriptor("Timestamp"),
            Value::Object(BTreeMap::from([(
                "morning".into(),
                Value::Timestamp(DateTime::from_naive_utc_and_offset(
                    NaiveDateTime::from_timestamp_opt(8675, 309).unwrap(),
                    Utc,
                )),
            )])),
        )
        .unwrap();
        let timestamp = mfield!(message, "morning").as_message().unwrap().clone();
        assert_eq!(Some(8675), mfield!(timestamp, "seconds").as_i64());
        assert_eq!(Some(309), mfield!(timestamp, "nanos").as_i32());
    }

    #[test]
    fn test_encode_repeated_primitive() {
        let message = encode_message(
            &test_message_descriptor("RepeatedPrimitive"),
            Value::Object(BTreeMap::from([(
                "numbers".into(),
                Value::Array(vec![
                    Value::Integer(8),
                    Value::Integer(6),
                    Value::Integer(4),
                ]),
            )])),
        )
        .unwrap();
        let list = mfield!(message, "numbers").as_list().unwrap().to_vec();
        assert_eq!(3, list.len());
        assert_eq!(Some(8), list[0].as_i64());
        assert_eq!(Some(6), list[1].as_i64());
        assert_eq!(Some(4), list[2].as_i64());
    }

    #[test]
    fn test_encode_repeated_message() {
        let message = encode_message(
            &test_message_descriptor("RepeatedMessage"),
            Value::Object(BTreeMap::from([(
                "messages".into(),
                Value::Array(vec![
                    Value::Object(BTreeMap::from([(
                        "text".into(),
                        Value::Bytes(Bytes::from("vector")),
                    )])),
                    Value::Object(BTreeMap::from([("index".into(), Value::Integer(4444))])),
                    Value::Object(BTreeMap::from([
                        ("text".into(), Value::Bytes(Bytes::from("protobuf"))),
                        ("index".into(), Value::Integer(1)),
                    ])),
                ]),
            )])),
        )
        .unwrap();
        let list = mfield!(message, "messages").as_list().unwrap().to_vec();
        assert_eq!(3, list.len());
        assert_eq!(
            Some("vector"),
            mfield!(list[0].as_message().unwrap(), "text").as_str()
        );
        assert!(!list[0].as_message().unwrap().has_field_by_name("index"));
        assert!(!list[1].as_message().unwrap().has_field_by_name("t4ext"));
        assert_eq!(
            Some(4444),
            mfield!(list[1].as_message().unwrap(), "index").as_u32()
        );
        assert_eq!(
            Some("protobuf"),
            mfield!(list[2].as_message().unwrap(), "text").as_str()
        );
        assert_eq!(
            Some(1),
            mfield!(list[2].as_message().unwrap(), "index").as_u32()
        );
    }

    fn run_encoding_on_decoding_test_data(
        filename: &str,
        message_type: &str,
    ) -> Result<DynamicMessage, vector_common::Error> {
        let protos_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("tests/data/protobuf/protos");
        let descriptor_set_path = protos_dir.join(filename);
        let message_descriptor =
            get_message_descriptor(&descriptor_set_path, message_type).unwrap();
        encode_message(
            &message_descriptor,
            Value::Object(BTreeMap::from([
                ("name".into(), Value::Bytes(Bytes::from("rope"))),
                ("id".into(), Value::Integer(9271)),
            ])),
        )
    }

    #[test]
    fn test_encode_decoding_protobuf_test_data() {
        // just check for the side-effect of success
        run_encoding_on_decoding_test_data("test_protobuf.desc", "test_protobuf.Person").unwrap();
        run_encoding_on_decoding_test_data("test_protobuf3.desc", "test_protobuf3.Person").unwrap();
    }
}
