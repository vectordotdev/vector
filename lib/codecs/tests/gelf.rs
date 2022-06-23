use bytes::Bytes;
use chrono::{DateTime, NaiveDateTime, Utc};
use codecs::{
    decoding::format::gelf_fields::*, decoding::format::Deserializer, GelfDeserializerConfig,
};
use lookup::path;
use pretty_assertions::assert_eq;
use serde_json::json;
use smallvec::SmallVec;
use value::Value;
use vector_core::{config::log_schema, event::Event};

fn deserialize_gelf_input(input: &serde_json::Value) -> vector_core::Result<SmallVec<[Event; 1]>> {
    let config = GelfDeserializerConfig;
    let deserializer = config.build();
    let buffer = Bytes::from(serde_json::to_vec(&input).unwrap());
    deserializer.parse(buffer)
}

/// Validates all the spec'd fields of GELF are deserialized correctly.
#[test]
fn gelf_deserialize_correctness() {
    let add_on_int_in = "_an.add-field_int";
    let add_on_str_in = "_an.add-field_str";

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
        add_on_int_in: 2001.1002,
        add_on_str_in: "A Space Odyssey",
    });

    // Ensure that we can parse the gelf json successfully
    let events = deserialize_gelf_input(&input).unwrap();
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
    assert_eq!(
        log.get(path!(&add_on_int_in[1..])),
        Some(&Value::Float(
            ordered_float::NotNan::new(2001.1002).unwrap()
        ))
    );
    assert_eq!(
        log.get(path!(&add_on_str_in[1..])),
        Some(&Value::Bytes(Bytes::from_static(b"A Space Odyssey")))
    );
}

/// Validates deserializiation succeeds for edge case inputs.
#[test]
fn gelf_deserializing_edge_cases() {
    // host is not specified
    {
        let input = json!({
            SHORT_MESSAGE: "foobar",
        });
        assert!(deserialize_gelf_input(&input).is_ok());
    }

    //  message set instead of short_message
    {
        let input = json!({
            "message": "foobar",
        });
        assert!(deserialize_gelf_input(&input).is_ok());
    }

    //  timestamp is wrong type
    {
        let input = json!({
            "message": "foobar",
            TIMESTAMP: "hammer time",
        });
        assert!(deserialize_gelf_input(&input).is_ok());
    }

    //  level / line
    {
        let input = json!({
            "message": "foobar",
            LINE: "-1",
        });
        assert!(deserialize_gelf_input(&input).is_ok());
    }
    {
        let input = json!({
            "message": "foobar",
            LEVEL: "4.2",
        });
        assert!(deserialize_gelf_input(&input).is_ok());
    }
    {
        let input = json!({
            "message": "foobar",
            LEVEL: 4.2,
        });
        assert!(deserialize_gelf_input(&input).is_ok());
    }

    //  invalid character in field name - field is dropped
    {
        let bad_key = "_invalid$field%name";
        let input = json!({
            "message": "foobar",
            bad_key: "drop_me",
        });
        let events = deserialize_gelf_input(&input).unwrap();
        assert_eq!(events.len(), 1);
        let log = events[0].as_log();
        assert!(!log.contains(bad_key));
    }
}

/// Validates the error conditions in deserialization
#[test]
fn gelf_deserializing_err() {
    fn validate_err(input: &serde_json::Value) {
        assert!(deserialize_gelf_input(input).is_err());
    }

    // host is not a string
    validate_err(&json!({
        HOST: 42,
        SHORT_MESSAGE: "foobar",
    }));

    // missing message and short_message
    validate_err(&json!({
        HOST: "example.org",
    }));

    //  level / line is string and not numeric
    validate_err(&json!({
        SHORT_MESSAGE: "foobar",
        LEVEL: "baz",
    }));
}
