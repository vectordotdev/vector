//! Tests for the behaviour of Protobuf serializer and deserializer (together).

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use bytes::{Bytes, BytesMut};
use codecs::{
    decoding::{
        ProtobufDeserializer, ProtobufDeserializerConfig, ProtobufDeserializerOptions,
        format::Deserializer,
    },
    encoding::{ProtobufSerializer, ProtobufSerializerConfig, ProtobufSerializerOptions},
};
use tokio_util::codec::Encoder;
use vector_core::config::LogNamespace;

fn test_data_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("tests/data/protobuf")
}

fn read_protobuf_bin_message(path: &Path) -> Bytes {
    let message_raw = std::fs::read(path).unwrap();
    Bytes::copy_from_slice(&message_raw)
}

/// Build the serializer and deserializer from common settings
fn build_serializer_pair(
    desc_file: PathBuf,
    message_type: String,
    use_json_names: bool,
) -> (ProtobufSerializer, ProtobufDeserializer) {
    let serializer = ProtobufSerializerConfig {
        protobuf: ProtobufSerializerOptions {
            desc_file: desc_file.clone(),
            message_type: message_type.clone(),
            use_json_names,
        },
    }
    .build()
    .unwrap();
    let deserializer = ProtobufDeserializerConfig {
        protobuf: ProtobufDeserializerOptions {
            desc_file,
            message_type,
            use_json_names,
        },
    }
    .build()
    .unwrap();
    (serializer, deserializer)
}

#[test]
fn roundtrip_coding() {
    let protobuf_message =
        read_protobuf_bin_message(&test_data_dir().join("pbs/person_someone.pb"));
    let desc_file = test_data_dir().join("protos/test_protobuf.desc");
    let message_type: String = "test_protobuf.Person".into();
    let (mut serializer, deserializer) = build_serializer_pair(desc_file, message_type, false);

    let events_original = deserializer
        .parse(protobuf_message, LogNamespace::Vector)
        .unwrap();
    assert_eq!(1, events_original.len());
    let mut new_message = BytesMut::new();
    serializer
        .encode(events_original[0].clone(), &mut new_message)
        .unwrap();
    let protobuf_message: Bytes = new_message.into();
    let events_encoded = deserializer
        .parse(protobuf_message, LogNamespace::Vector)
        .unwrap();
    assert_eq!(events_original, events_encoded);
}

#[test]
fn roundtrip_coding_with_json_names() {
    let protobuf_message =
        read_protobuf_bin_message(&test_data_dir().join("pbs/person_someone3.pb"));
    let desc_file = test_data_dir().join("protos/test_protobuf3.desc");
    let message_type: String = "test_protobuf3.Person".into();

    // Test with use_json_names=false (default behavior - snake_case field names)
    let (mut serializer_snake_case, deserializer_snake_case) =
        build_serializer_pair(desc_file.clone(), message_type.clone(), false);

    let events_snake_case = deserializer_snake_case
        .parse(protobuf_message.clone(), LogNamespace::Vector)
        .unwrap();
    assert_eq!(1, events_snake_case.len());

    // Verify that protobuf field names are being used (snake_case)
    let event = events_snake_case[0].as_log();
    assert!(
        event.contains("job_description"),
        "Event should contain 'job_description' (protobuf field name) when use_json_names is disabled"
    );
    assert_eq!(
        event.get("job_description").unwrap().to_string_lossy(),
        "Software Engineer"
    );
    assert!(
        !event.contains("jobDescription"),
        "Event should not contain 'jobDescription' (JSON name) when use_json_names is disabled"
    );

    // Test roundtrip with snake_case
    let mut new_message = BytesMut::new();
    serializer_snake_case
        .encode(events_snake_case[0].clone(), &mut new_message)
        .unwrap();
    let events_encoded = deserializer_snake_case
        .parse(new_message.into(), LogNamespace::Vector)
        .unwrap();
    assert_eq!(events_snake_case, events_encoded);

    // Test with use_json_names=true (camelCase field names)
    let (mut serializer_camel_case, deserializer_camel_case) =
        build_serializer_pair(desc_file, message_type, true);

    let events_camel_case = deserializer_camel_case
        .parse(protobuf_message, LogNamespace::Vector)
        .unwrap();
    assert_eq!(1, events_camel_case.len());

    // Verify that JSON names are being used (camelCase)
    let event = events_camel_case[0].as_log();
    assert!(
        event.contains("jobDescription"),
        "Event should contain 'jobDescription' (JSON name) when use_json_names is enabled"
    );
    assert_eq!(
        event.get("jobDescription").unwrap().to_string_lossy(),
        "Software Engineer"
    );
    assert!(
        !event.contains("job_description"),
        "Event should not contain 'job_description' (protobuf name) when use_json_names is enabled"
    );

    // Test roundtrip with camelCase
    let mut new_message = BytesMut::new();
    serializer_camel_case
        .encode(events_camel_case[0].clone(), &mut new_message)
        .unwrap();
    let events_encoded = deserializer_camel_case
        .parse(new_message.into(), LogNamespace::Vector)
        .unwrap();
    assert_eq!(events_camel_case, events_encoded);
}
