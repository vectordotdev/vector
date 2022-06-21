use bytes::Bytes;
use chrono::{DateTime, NaiveDateTime, Utc};
use codecs::{
    decoding::format::gelf_fields::*, decoding::format::Deserializer, GelfDeserializerConfig,
};
use pretty_assertions::assert_eq;
use serde_json::json;
use value::Value;
use vector_core::config::log_schema;

/// Validates all the spec'd fields of GELF are deserialized.
#[test]
fn gelf_deserializing_all() {
    let config = GelfDeserializerConfig;
    let deserializer = config.build();

    let add_on_int_in = "_an.add-field_int";
    let add_on_str_in = "_an.add-field_str";
    let add_on_int_out = "_an_add_field_int";
    let add_on_str_out = "_an_add_field_str";

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
        add_on_int_in: 2001,
        add_on_str_in: "A Space Odyssey",
    });

    let buffer = Bytes::from(serde_json::to_vec(&input).unwrap());

    // Ensure that we can parse the gelf json successfully
    let events = deserializer.parse(buffer.clone()).unwrap();
    assert_eq!(events.len(), 1);

    let log = events[0].as_log();

    assert_eq!(
        log.get(VERSION),
        Some(&Value::Bytes(Bytes::from_static(b"1.1")))
    );
    assert_eq!(
        log.get(HOST),
        Some(&Value::Bytes(Bytes::from_static(b"example.org")))
    );
    assert_eq!(
        log.get(log_schema().message_key()),
        Some(&Value::Bytes(Bytes::from_static(
            b"A short message that helps you identify what is going on"
        )))
    );
    assert_eq!(
        log.get(FULL_MESSAGE),
        Some(&Value::Bytes(Bytes::from_static(
            b"Backtrace here\n\nmore stuff"
        )))
    );
    // Vector does not use the nanos
    let naive = NaiveDateTime::from_timestamp(1385053862, 0);
    assert_eq!(
        log.get(TIMESTAMP),
        Some(&Value::Timestamp(DateTime::<Utc>::from_utc(naive, Utc)))
    );
    assert_eq!(log.get(LEVEL), Some(&Value::Integer(1)));
    assert_eq!(
        log.get(FACILITY),
        Some(&Value::Bytes(Bytes::from_static(b"foo")))
    );
    assert_eq!(log.get(LINE), Some(&Value::Integer(42)));
    assert_eq!(
        log.get(FILE),
        Some(&Value::Bytes(Bytes::from_static(b"/tmp/bar")))
    );
    assert_eq!(log.get(add_on_int_out), Some(&Value::Integer(2001)));
    assert_eq!(
        log.get(add_on_str_out),
        Some(&Value::Bytes(Bytes::from_static(b"A Space Odyssey")))
    );
}

/// Validates the error conditions in deserialization
#[test]
fn gelf_deserializing_err() {
    fn validate_err(input: &serde_json::Value) {
        let config = GelfDeserializerConfig;
        let deserializer = config.build();
        let buffer = Bytes::from(serde_json::to_vec(&input).unwrap());
        deserializer.parse(buffer.clone()).unwrap_err();
    }

    // missing SHORT_MESSAGE
    validate_err(&json!({
        VERSION: "1.1",
        HOST: "example.org",
    }));
    // missing HOST
    validate_err(&json!({
        VERSION: "1.1",
        SHORT_MESSAGE: "A short message that helps you identify what is going on",
    }));
    // missing VERSION
    validate_err(&json!({
        HOST: "example.org",
        SHORT_MESSAGE: "A short message that helps you identify what is going on",
    }));
}
