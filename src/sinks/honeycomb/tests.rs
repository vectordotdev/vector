//! Unit tests for the `honeycomb` sink.

use futures::{future::ready, stream};
use serde::Deserialize;

use super::config::HoneycombConfig;
use crate::{
    sinks::prelude::*,
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<HoneycombConfig>();
}

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let config = HoneycombConfig::generate_config().to_string();
    let mut config = HoneycombConfig::deserialize(
        toml::de::ValueDeserializer::parse(&config).expect("toml should deserialize"),
    )
    .expect("config should be valid");
    config.endpoint = mock_endpoint.to_string();

    let context = SinkContext::default();
    let (sink, _healthcheck) = config.build(context).await.unwrap();

    let event = Event::Log(LogEvent::from("simple message"));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
}

mod samplerate {
    use std::io::Cursor;

    use super::*;
    use crate::sinks::{
        honeycomb::encoder::HoneycombEncoder, util::encoding::Encoder as SinkEncoder,
    };
    use vector_lib::lookup::lookup_v2::OptionalTargetPath;

    fn make_encoder(samplerate_field: Option<&str>) -> HoneycombEncoder {
        HoneycombEncoder {
            transformer: Transformer::default(),
            samplerate_field: samplerate_field.map(|f| {
                OptionalTargetPath::try_from(f.to_string())
                    .expect("unable to parse OptionalTargetPath")
            }),
        }
    }

    fn decode_output(buf: &[u8]) -> Vec<serde_json::Value> {
        serde_json::from_slice(buf).expect("should be valid JSON array")
    }

    #[test]
    fn encode_event_with_samplerate_field() {
        let encoder = make_encoder(Some("rate"));

        let mut log = LogEvent::default();
        log.insert("rate", 5);
        log.insert("message", "hello");

        let mut buf = Cursor::new(Vec::new());
        encoder
            .encode_input(vec![Event::Log(log)], &mut buf)
            .unwrap();

        let output = decode_output(buf.get_ref());
        assert_eq!(output.len(), 1);

        let event = &output[0];
        assert_eq!(event["samplerate"], serde_json::json!(5));

        // "rate" should be removed from data
        assert!(event["data"].get("rate").is_none());
        // "message" should still be in data
        assert!(event["data"].get("message").is_some());
    }

    #[test]
    fn encode_event_without_samplerate_field_configured() {
        let encoder = make_encoder(None);

        let mut log = LogEvent::default();
        log.insert("rate", 5);
        log.insert("message", "hello");

        let mut buf = Cursor::new(Vec::new());
        encoder
            .encode_input(vec![Event::Log(log)], &mut buf)
            .unwrap();

        let output = decode_output(buf.get_ref());
        let event = &output[0];

        // No samplerate key at top level
        assert!(event.get("samplerate").is_none());
        // "rate" should still be in data
        assert!(event["data"].get("rate").is_some());
    }

    #[test]
    fn encode_event_samplerate_field_missing_from_event() {
        let encoder = make_encoder(Some("rate"));

        let mut log = LogEvent::default();
        log.insert("message", "hello");

        let mut buf = Cursor::new(Vec::new());
        encoder
            .encode_input(vec![Event::Log(log)], &mut buf)
            .unwrap();

        let output = decode_output(buf.get_ref());
        let event = &output[0];

        // No samplerate key since the field doesn't exist
        assert!(event.get("samplerate").is_none());
    }

    #[test]
    fn encode_event_samplerate_field_not_integer() {
        let encoder = make_encoder(Some("rate"));

        let mut log = LogEvent::default();
        log.insert("rate", "not-a-number");
        log.insert("message", "hello");

        let mut buf = Cursor::new(Vec::new());
        encoder
            .encode_input(vec![Event::Log(log)], &mut buf)
            .unwrap();

        let output = decode_output(buf.get_ref());
        let event = &output[0];

        // samplerate should be omitted for non-integer values
        assert!(event.get("samplerate").is_none());
        // Configured field is always removed from data
        assert!(event["data"].get("rate").is_none());
    }

    #[test]
    fn encode_event_samplerate_field_value_zero_or_negative() {
        let encoder = make_encoder(Some("rate"));

        // Zero is invalid (samplerate means "1 in N", N must be > 0)
        let mut log = LogEvent::default();
        log.insert("rate", 0);
        log.insert("message", "hello");

        let mut buf = Cursor::new(Vec::new());
        encoder
            .encode_input(vec![Event::Log(log)], &mut buf)
            .unwrap();

        let output = decode_output(buf.get_ref());
        let event = &output[0];

        assert!(event.get("samplerate").is_none());
        // Invalid integer values are still removed from data
        assert!(event["data"].get("rate").is_none());

        // Negative is also invalid
        let mut log = LogEvent::default();
        log.insert("rate", -5);
        log.insert("message", "hello");

        let mut buf = Cursor::new(Vec::new());
        encoder
            .encode_input(vec![Event::Log(log)], &mut buf)
            .unwrap();

        let output = decode_output(buf.get_ref());
        let event = &output[0];

        assert!(event.get("samplerate").is_none());
        assert!(event["data"].get("rate").is_none());
    }

    #[test]
    fn encode_multiple_events_mixed_samplerate() {
        let encoder = make_encoder(Some("rate"));

        let mut log1 = LogEvent::default();
        log1.insert("rate", 10);
        log1.insert("message", "with rate");

        let mut log2 = LogEvent::default();
        log2.insert("message", "no rate");

        let mut log3 = LogEvent::default();
        log3.insert("rate", "invalid");
        log3.insert("message", "invalid rate");

        let mut buf = Cursor::new(Vec::new());
        encoder
            .encode_input(
                vec![Event::Log(log1), Event::Log(log2), Event::Log(log3)],
                &mut buf,
            )
            .unwrap();

        let output = decode_output(buf.get_ref());
        assert_eq!(output.len(), 3);

        // First event: has integer samplerate
        assert_eq!(output[0]["samplerate"], serde_json::json!(10));
        assert!(output[0]["data"].get("rate").is_none());

        // Second event: no samplerate field at all
        assert!(output[1].get("samplerate").is_none());

        // Third event: non-integer, so no samplerate; field still removed from data
        assert!(output[2].get("samplerate").is_none());
        assert!(output[2]["data"].get("rate").is_none());
    }
}
