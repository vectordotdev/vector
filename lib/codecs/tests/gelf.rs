use bytes::Bytes;
use codecs::{
    decoding::format::gelf_fields::*, decoding::format::Deserializer, GelfDeserializerConfig,
};
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn gelf_deserializing() {
    let config = GelfDeserializerConfig;
    let deserializer = config.build();

    let input = json!({
        VERSION: "1.1",
        HOST: "example.org",
        SHORT_MESSAGE: "A short message that helps you identify what is going on",
        FULL_MESSAGE: "Backtrace here\n\nmore stuff",
        TIMESTAMP: 1385053862.3072,
        LEVEL: 1,
        FACILITY: "foo",
        LINE: 42,
        FILE: "/tmp/bar",
    });

    let buffer = serde_json::to_vec(&input).unwrap();
    let buffer = Bytes::from(buffer);

    // Ensure that we can parse the gelf json successfully
    let events = deserializer.parse(buffer.clone()).unwrap();
    assert_eq!(events.len(), 1);
    dbg!(&events[0]);
}
