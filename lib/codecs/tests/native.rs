use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use bytes::{Bytes, BytesMut};
use codecs::{
    decoding::format::Deserializer, encoding::format::Serializer, NativeDeserializerConfig,
    NativeJsonDeserializerConfig, NativeJsonSerializerConfig, NativeSerializerConfig,
};
use similar_asserts::assert_eq;
use vector_core::{config::LogNamespace, event::Event};

#[test]
fn pre_v24_fixtures_match() {
    fixtures_match("pre-v24");
}

#[test]
fn pre_v34_fixtures_match() {
    fixtures_match("pre-v34");
}

#[test]
fn pre_v41_fixtures_match() {
    fixtures_match("pre-v41");
}

#[test]
fn current_fixtures_match() {
    fixtures_match("");
}

#[test]
fn roundtrip_current_native_json_fixtures() {
    roundtrip_fixtures(
        "json",
        "",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
        false,
    );
}

#[test]
fn roundtrip_current_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        false,
    );
}

/// The event proto file was changed in v0.24. This test ensures we can still load the old version
/// binary and that when serialized and deserialized in the new format we still get the same event.
#[test]
fn reserialize_pre_v24_native_json_fixtures() {
    roundtrip_fixtures(
        "json",
        "pre-v24",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
        true,
    );
}

#[test]
fn reserialize_pre_v24_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "pre-v24",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        true,
    );
}

/// The event proto format was changed in v26 to include support for enhanced metric tags. This test
/// ensures we can still load the old version binary and that when serialized and deserialized in
/// the new format we still get the same event.
#[test]
fn reserialize_pre_v26_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "pre-v26",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        true,
    );
}

/// The event proto file was changed in v0.34. This test ensures we can still load the old version
/// binary and that when serialized and deserialized in the new format we still get the same event.
#[test]
fn reserialize_pre_v34_native_json_fixtures() {
    roundtrip_fixtures(
        "json",
        "pre-v34",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
        true,
    );
}

#[test]
fn reserialize_pre_v34_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "pre-v34",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        true,
    );
}

/// The event proto file was changed in v0.41. This test ensures we can still load the old version
/// binary and that when serialized and deserialized in the new format we still get the same event.
#[test]
fn reserialize_pre_v41_native_json_fixtures() {
    roundtrip_fixtures(
        "json",
        "pre-v41",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
        true,
    );
}

#[test]
fn reserialize_pre_v41_native_proto_fixtures() {
    roundtrip_fixtures(
        "proto",
        "pre-v41",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
        true,
    );
}

// TODO: the json &  protobuf consistency has been broken for a while due to the lack of implementing
// serde deser and ser of EventMetadata. Thus the `native_json` codec is not passing through the
// `EventMetadata.value` field, whereas the `native` codec does.
//
// both of these tests are affected as a result
//
// https://github.com/vectordotdev/vector/issues/18570
#[ignore]
#[test]
fn pre_v34_native_decoding_matches() {
    decoding_matches("pre-v34");
}

#[ignore]
#[test]
fn pre_v41_native_decoding_matches() {
    decoding_matches("pre-v41");
}

#[ignore]
#[test]
fn current_native_decoding_matches() {
    decoding_matches("");
}

#[test]
fn pre_v24_native_decoding_matches() {
    decoding_matches("pre-v24");
}

/// This "test" can be used to build new protobuf fixture files when the protocol changes. Remove
/// the `#[ignore]` only when this is needed for such changes. You will need to manually create a
/// `tests/data/native_encoding/json/rebuilt` subdirectory for the files to be written to.
#[test]
#[ignore]
fn rebuild_json_fixtures() {
    rebuild_fixtures(
        "json",
        &NativeJsonDeserializerConfig::default().build(),
        &mut NativeJsonSerializerConfig.build(),
    );
}

/// This "test" can be used to build new protobuf fixture files when the protocol changes. Remove
/// the `#[ignore]` only when this is needed for such changes. You will need to manually create a
/// `tests/data/native_encoding/proto/rebuilt` subdirectory for the files to be written to.
#[test]
#[ignore]
fn rebuild_proto_fixtures() {
    rebuild_fixtures(
        "proto",
        &NativeDeserializerConfig.build(),
        &mut NativeSerializerConfig.build(),
    );
}

/// This test ensures that the different sets of protocol fixture names match.
fn fixtures_match(suffix: &str) {
    let json_entries = list_fixtures("json", suffix);
    let proto_entries = list_fixtures("proto", suffix);
    for (json_path, proto_path) in json_entries.into_iter().zip(proto_entries.into_iter()) {
        // Make sure we're looking at the matching files for each format
        assert_eq!(
            json_path.file_stem().unwrap(),
            proto_path.file_stem().unwrap(),
        );
    }
}

/// This test ensures we can load the serialized binaries binary and that they match across
/// protocols.
fn decoding_matches(suffix: &str) {
    let json_deserializer = NativeJsonDeserializerConfig::default().build();
    let proto_deserializer = NativeDeserializerConfig.build();

    let json_entries = list_fixtures("json", suffix);
    let proto_entries = list_fixtures("proto", suffix);

    for (json_path, proto_path) in json_entries.into_iter().zip(proto_entries.into_iter()) {
        let (_, json_event) = load_deserialize(&json_path, &json_deserializer);

        let (_, proto_event) = load_deserialize(&proto_path, &proto_deserializer);

        // Ensure that the json version and proto versions were parsed into equivalent
        // native representations
        assert_eq!(
            json_event,
            proto_event,
            "Parsed events don't match: {} {}",
            json_path.display(),
            proto_path.display()
        );
    }
}

fn list_fixtures(proto: &str, suffix: &str) -> Vec<PathBuf> {
    let path = fixtures_path(proto, suffix);
    let mut entries = fs::read_dir(path)
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.file_type().unwrap().is_file())
        .map(|e| e.path())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn fixtures_path(proto: &str, suffix: &str) -> PathBuf {
    ["tests/data/native_encoding", proto, suffix]
        .into_iter()
        .collect()
}

fn roundtrip_fixtures(
    proto: &str,
    suffix: &str,
    deserializer: &dyn Deserializer,
    serializer: &mut dyn Serializer,
    reserialize: bool,
) {
    for path in list_fixtures(proto, suffix) {
        let (buf, event) = load_deserialize(&path, deserializer);

        if reserialize {
            // Serialize the parsed event
            let mut buf = BytesMut::new();
            serializer.encode(event.clone(), &mut buf).unwrap();
            // Deserialize the event from these bytes
            let new_events = deserializer
                .parse(buf.into(), LogNamespace::Legacy)
                .unwrap();

            // Ensure we have the same event.
            assert_eq!(new_events.len(), 1);
            assert_eq!(new_events[0], event);
        } else {
            // Ensure that the parsed event is serialized to the same bytes
            let mut new_buf = BytesMut::new();
            serializer.encode(event.clone(), &mut new_buf).unwrap();
            assert_eq!(buf, new_buf);
        }
    }
}

fn load_deserialize(path: &Path, deserializer: &dyn Deserializer) -> (Bytes, Event) {
    let mut file = File::open(path).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    let buf = Bytes::from(buf);

    // Ensure that we can parse the json fixture successfully
    let mut events = deserializer
        .parse(buf.clone(), LogNamespace::Legacy)
        .unwrap();
    assert_eq!(events.len(), 1);
    (buf, events.pop().unwrap())
}

fn rebuild_fixtures(proto: &str, deserializer: &dyn Deserializer, serializer: &mut dyn Serializer) {
    for path in list_fixtures(proto, "") {
        let (_, event) = load_deserialize(&path, deserializer);

        let mut buf = BytesMut::new();
        serializer
            .encode(event, &mut buf)
            .expect("Serializing failed");

        let new_path: PathBuf = [
            fixtures_path(proto, "rebuilt"),
            path.file_name().unwrap().into(),
        ]
        .into_iter()
        .collect();
        let mut out = File::create(&new_path).unwrap_or_else(|error| {
            panic!("Could not create rebuilt file {:?}: {:?}", new_path, error)
        });
        out.write_all(&buf).expect("Could not write rebuilt data");
        out.flush().expect("Could not write rebuilt data");
    }
}
