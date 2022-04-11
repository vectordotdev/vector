use std::{
    fs::{self, File},
    io::Read,
};

use bytes::{Bytes, BytesMut};
use pretty_assertions::assert_eq;
use tokio_util::codec::Encoder;

use codecs::{
    decoding::format::Deserializer, NativeDeserializerConfig, NativeJsonDeserializerConfig,
    NativeJsonSerializerConfig, NativeSerializerConfig,
};

#[test]
fn roundtrip_native_fixtures() {
    let config = NativeJsonDeserializerConfig;
    let json_deserializer = config.build();

    let config = NativeDeserializerConfig;
    let proto_deserializer = config.build();

    let config = NativeJsonSerializerConfig;
    let mut json_serializer = config.build();

    let config = NativeSerializerConfig;
    let mut proto_serializer = config.build();

    let mut json_entries = fs::read_dir("tests/data/native_encoding/json/")
        .unwrap()
        .map(Result::unwrap)
        .map(|e| e.path())
        .collect::<Vec<_>>();
    json_entries.sort();

    let mut proto_entries = fs::read_dir("tests/data/native_encoding/proto/")
        .unwrap()
        .map(Result::unwrap)
        .map(|e| e.path())
        .collect::<Vec<_>>();
    proto_entries.sort();

    for (json_path, proto_path) in json_entries.into_iter().zip(proto_entries.into_iter()) {
        // Make sure we're looking at the matching files for each format
        assert_eq!(
            json_path.file_stem().unwrap(),
            proto_path.file_stem().unwrap(),
        );

        let mut json_file = File::open(json_path).unwrap();
        let mut json_buf = Vec::new();
        json_file.read_to_end(&mut json_buf).unwrap();
        let json_buf = Bytes::from(json_buf);

        // Ensure that we can parse the json fixture successfully
        let json_events = json_deserializer.parse(json_buf.clone()).unwrap();
        assert_eq!(json_events.len(), 1);

        // Ensure that the parsed event is serialized to the same bytes
        let mut buf = BytesMut::new();
        json_serializer
            .encode(json_events[0].clone(), &mut buf)
            .unwrap();
        assert_eq!(json_buf, buf);

        let mut proto_file = File::open(proto_path).unwrap();
        let mut proto_buf = Vec::new();
        proto_file.read_to_end(&mut proto_buf).unwrap();
        let proto_buf = Bytes::from(proto_buf);

        // Ensure that we can parse the proto fixture successfully
        let proto_events = proto_deserializer.parse(proto_buf.clone()).unwrap();
        assert_eq!(proto_events.len(), 1);

        // Ensure that the parsed event is serialized to the same bytes
        let mut buf = BytesMut::new();
        proto_serializer
            .encode(proto_events[0].clone(), &mut buf)
            .unwrap();
        assert_eq!(proto_buf, buf);

        // Ensure that the json version and proto versions were parsed into equivalent
        // native representations
        assert_eq!(json_events, proto_events);
    }
}
