#![cfg(test)]

use super::*;
use std::collections::BTreeMap;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<KinesisFirehoseSinkConfig>();
}

#[test]
fn check_batch_size() {
    let config = KinesisFirehoseSinkConfig {
        stream_name: String::from("test"),
        region: RegionOrEndpoint::with_endpoint("http://localhost:4566".into()),
        encoding: EncodingConfig::from(Encoding::Json),
        compression: Compression::None,
        batch: BatchConfig {
            max_bytes: Some(MAX_PAYLOAD_SIZE + 1),
            ..Default::default()
        },
        request: Default::default(),
        assume_role: None,
        auth: Default::default(),
    };

    let cx = SinkContext::new_test();

    let client = config.create_client(&cx.proxy).unwrap();

    let res = KinesisFirehoseService::new(config, client, cx);

    assert_eq!(
        res.err().and_then(|e| e.downcast::<BuildError>().ok()),
        Some(Box::new(BuildError::BatchMaxSize))
    );
}

#[test]
fn check_batch_events() {
    let config = KinesisFirehoseSinkConfig {
        stream_name: String::from("test"),
        region: RegionOrEndpoint::with_endpoint("http://localhost:4566".into()),
        encoding: EncodingConfig::from(Encoding::Json),
        compression: Compression::None,
        batch: BatchConfig {
            max_events: Some(MAX_PAYLOAD_EVENTS + 1),
            ..Default::default()
        },
        request: Default::default(),
        assume_role: None,
        auth: Default::default(),
    };

    let cx = SinkContext::new_test();

    let client = config.create_client(&cx.proxy).unwrap();

    let res = KinesisFirehoseService::new(config, client, cx);

    assert_eq!(
        res.err().and_then(|e| e.downcast::<BuildError>().ok()),
        Some(Box::new(BuildError::BatchMaxEvents))
    );
}

#[test]
fn firehose_encode_event_text() {
    let message = "hello world".to_string();
    let event = encode_event(message.clone().into(), &Encoding::Text.into());

    assert_eq!(&event.item.data[..], message.as_bytes());
}

#[test]
fn firehose_encode_event_json() {
    let message = "hello world".to_string();
    let mut event = Event::from(message.clone());
    event.as_mut_log().insert("key", "value");
    let event = encode_event(event, &Encoding::Json.into());

    let map: BTreeMap<String, String> = serde_json::from_slice(&event.item.data[..]).unwrap();

    assert_eq!(
        map[&crate::config::log_schema().message_key().to_string()],
        message
    );
    assert_eq!(map["key"], "value".to_string());
}
