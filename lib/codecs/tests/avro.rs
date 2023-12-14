use bytes::{Bytes, BytesMut};
use codecs::{
    decoding::format::AvroDeserializerConfig, decoding::format::Deserializer,
    encoding::format::AvroSerializerConfig,
};
use rstest::*;
use similar_asserts::assert_eq;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    str::from_utf8,
};
use tokio_util::codec::Encoder;
use vector_core::{config::LogNamespace, event::Event};

#[rstest]
#[case(true)]
#[case(false)]
fn roundtrip_avro_fixtures(
    #[files("tests/data/avro/generated/*.avro")]
    #[exclude(".*(date|fixed|time_millis).avro")]
    path: PathBuf,
    #[case] reserialize: bool,
) {
    let schema_path = path.as_path().with_extension("avsc");
    assert!(schema_path.exists());

    roundtrip_avro(path, schema_path, reserialize);
}

fn roundtrip_avro(data_path: PathBuf, schema_path: PathBuf, reserialize: bool) {
    let schema = load_file(&schema_path);
    let schema = from_utf8(&schema).unwrap().to_string();
    let deserializer = AvroDeserializerConfig::new(schema.clone(), false).build();
    let mut serializer = AvroSerializerConfig::new(schema.clone()).build().unwrap();

    let (buf, event) = load_deserialize(&data_path, &deserializer);

    if reserialize {
        // Serialize the parsed event
        let mut buf = BytesMut::new();
        serializer.encode(event.clone(), &mut buf).unwrap();
        // Deserialize the event from these bytes
        let new_events = deserializer
            .parse(buf.into(), LogNamespace::Vector)
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

fn load_file(path: &Path) -> Bytes {
    let mut file = File::open(path).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    Bytes::from(buf)
}

fn load_deserialize(path: &Path, deserializer: &dyn Deserializer) -> (Bytes, Event) {
    let buf = load_file(path);

    let mut events = deserializer
        .parse(buf.clone(), LogNamespace::Vector)
        .unwrap();
    assert_eq!(events.len(), 1);
    (buf, events.pop().unwrap())
}
