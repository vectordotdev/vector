//! Tests for the behaviour of Protobuf serializer and deserializer (together).

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
) -> (ProtobufSerializer, ProtobufDeserializer) {
    let serializer = ProtobufSerializerConfig {
        protobuf: ProtobufSerializerOptions {
            desc_file: desc_file.clone(),
            message_type: message_type.clone(),
            use_json_names: false,
        },
    }
    .build()
    .unwrap();
    let deserializer = ProtobufDeserializerConfig {
        protobuf: ProtobufDeserializerOptions {
            desc_file,
            message_type,
            use_json_names: false,
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
    let (mut serializer, deserializer) = build_serializer_pair(desc_file, message_type);

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

    // Create serializer with use_json_names enabled
    let mut serializer = ProtobufSerializerConfig {
        protobuf: ProtobufSerializerOptions {
            desc_file: desc_file.clone(),
            message_type: message_type.clone(),
            use_json_names: true,
        },
    }
    .build()
    .unwrap();

    // Create deserializer with use_json_names enabled
    let deserializer = ProtobufDeserializerConfig {
        protobuf: ProtobufDeserializerOptions {
            desc_file,
            message_type,
            use_json_names: true,
        },
    }
    .build()
    .unwrap();

    // Parse the original message
    let events_original = deserializer
        .parse(protobuf_message, LogNamespace::Vector)
        .unwrap();
    assert_eq!(1, events_original.len());

    // Encode it back
    let mut new_message = BytesMut::new();
    serializer
        .encode(events_original[0].clone(), &mut new_message)
        .unwrap();

    // Decode the re-encoded message
    let protobuf_message: Bytes = new_message.into();
    let events_encoded = deserializer
        .parse(protobuf_message, LogNamespace::Vector)
        .unwrap();

    // Should be the same
    assert_eq!(events_original, events_encoded);
}
