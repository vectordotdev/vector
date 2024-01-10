use std::{convert::TryFrom, iter, num::NonZeroU8};

use chrono::{TimeZone, Timelike, Utc};
use futures::{future::ready, stream};
use serde_json::Value as JsonValue;
use tokio::time::{sleep, Duration};
use vector_lib::codecs::{JsonSerializerConfig, TextSerializerConfig};
use vector_lib::lookup::lookup_v2::{ConfigValuePath, OptionalTargetPath};
use vector_lib::{
    config::{init_telemetry, Tags, Telemetry},
    event::{BatchNotifier, BatchStatus, Event, LogEvent},
    lookup,
};
use vrl::path::OwnedTargetPath;

use crate::{
    codecs::EncodingConfig,
    config::{SinkConfig, SinkContext},
    event::Value,
    sinks::{
        splunk_hec::{
            common::{
                acknowledgements::HecClientAcknowledgementsConfig,
                integration_test_helpers::{get_token, splunk_api_address, splunk_hec_address},
                EndpointTarget, SOURCE_FIELD,
            },
            logs::config::HecLogsSinkConfig,
        },
        util::{BatchConfig, Compression, TowerRequestConfig},
    },
    template::Template,
    test_util::{
        components::{
            run_and_assert_data_volume_sink_compliance, run_and_assert_sink_compliance,
            DATA_VOLUME_SINK_TAGS, HTTP_SINK_TAGS,
        },
        random_lines_with_stream, random_string,
    },
};

const USERNAME: &str = "admin";
const PASSWORD: &str = "password";
const ACK_TOKEN: &str = "ack-token";

async fn recent_entries(index: Option<&str>) -> Vec<JsonValue> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    // https://docs.splunk.com/Documentation/Splunk/7.2.1/RESTREF/RESTsearch#search.2Fjobs
    let search_query = match index {
        Some(index) => format!("search index={}", index),
        None => "search index=*".into(),
    };
    let res = client
        .post(format!(
            "{}/services/search/jobs?output_mode=json",
            splunk_api_address()
        ))
        .form(&vec![
            ("search", &search_query[..]),
            ("exec_mode", "oneshot"),
            ("f", "*"),
        ])
        .basic_auth(USERNAME, Some(PASSWORD))
        .send()
        .await
        .unwrap();
    let json: JsonValue = res.json().await.unwrap();

    json["results"].as_array().unwrap().clone()
}

// It usually takes ~1 second for the event to show up in search, so poll until
// we see it.
async fn find_entry(message: &str) -> serde_json::value::Value {
    for _ in 0..20usize {
        match recent_entries(None)
            .await
            .into_iter()
            .find(|entry| entry["_raw"].as_str().unwrap_or("").contains(message))
        {
            Some(value) => return value,
            None => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
    panic!("Didn't find event in Splunk");
}

async fn find_entries(messages: &[String]) -> bool {
    let mut found_all = false;
    for _ in 0..20 {
        let entries = recent_entries(None).await;

        found_all = messages.iter().all(|message| {
            entries
                .iter()
                .any(|entry| entry["_raw"].as_str().unwrap().contains(message.as_str()))
        });

        if found_all {
            break;
        }

        sleep(Duration::from_millis(200)).await;
    }
    found_all
}

async fn config(
    encoding: EncodingConfig,
    indexed_fields: Vec<ConfigValuePath>,
) -> HecLogsSinkConfig {
    let mut batch = BatchConfig::default();
    batch.max_events = Some(5);

    HecLogsSinkConfig {
        default_token: get_token().await.into(),
        endpoint: splunk_hec_address(),
        host_key: OptionalTargetPath::event("host"),
        indexed_fields,
        index: None,
        sourcetype: None,
        source: None,
        encoding,
        compression: Compression::None,
        batch,
        request: TowerRequestConfig::default(),
        tls: None,
        acknowledgements: Default::default(),
        timestamp_nanos_key: None,
        timestamp_key: Default::default(),
        auto_extract_timestamp: None,
        endpoint_target: EndpointTarget::Event,
    }
}

fn enable_telemetry() {
    init_telemetry(
        Telemetry {
            tags: Tags {
                emit_service: true,
                emit_source: true,
            },
        },
        true,
    );
}

#[tokio::test]
async fn splunk_insert_message() {
    let cx = SinkContext::default();

    let config = config(TextSerializerConfig::default().into(), vec![]).await;
    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let event = LogEvent::from(message.clone()).with_batch_notifier(&batch);
    drop(batch);
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let entry = find_entry(message.as_str()).await;

    assert_eq!(message, entry["_raw"].as_str().unwrap());
    assert!(entry.get("message").is_none());
}

#[tokio::test]
async fn splunk_insert_message_data_volume() {
    enable_telemetry();

    let cx = SinkContext::default();

    let config = config(TextSerializerConfig::default().into(), vec![]).await;
    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let event = LogEvent::from(message.clone()).with_batch_notifier(&batch);
    drop(batch);
    run_and_assert_data_volume_sink_compliance(
        sink,
        stream::once(ready(event)),
        &DATA_VOLUME_SINK_TAGS,
    )
    .await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let entry = find_entry(message.as_str()).await;

    assert_eq!(message, entry["_raw"].as_str().unwrap());
    assert!(entry.get("message").is_none());
}

#[tokio::test]
async fn splunk_insert_raw_message() {
    let cx = SinkContext::default();

    let config = HecLogsSinkConfig {
        endpoint_target: EndpointTarget::Raw,
        source: Some(Template::try_from("zork").unwrap()),
        ..config(TextSerializerConfig::default().into(), vec![]).await
    };
    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let event = LogEvent::from(message.clone()).with_batch_notifier(&batch);
    drop(batch);
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let entry = find_entry(message.as_str()).await;

    assert_eq!(message, entry["_raw"].as_str().unwrap());
    assert_eq!("zork", entry[SOURCE_FIELD].as_str().unwrap());
    assert!(entry.get("message").is_none());
}

#[tokio::test]
async fn splunk_insert_raw_message_data_volume() {
    enable_telemetry();

    let cx = SinkContext::default();

    let config = HecLogsSinkConfig {
        endpoint_target: EndpointTarget::Raw,
        source: Some(Template::try_from("zork").unwrap()),
        ..config(TextSerializerConfig::default().into(), vec![]).await
    };
    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let event = LogEvent::from(message.clone()).with_batch_notifier(&batch);
    drop(batch);
    run_and_assert_data_volume_sink_compliance(
        sink,
        stream::once(ready(event)),
        &DATA_VOLUME_SINK_TAGS,
    )
    .await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let entry = find_entry(message.as_str()).await;

    assert_eq!(message, entry["_raw"].as_str().unwrap());
    assert_eq!("zork", entry[SOURCE_FIELD].as_str().unwrap());
    assert!(entry.get("message").is_none());
}

#[tokio::test]
async fn splunk_insert_broken_token() {
    let cx = SinkContext::default();

    let mut config = config(TextSerializerConfig::default().into(), vec![]).await;
    config.default_token = "BROKEN_TOKEN".to_string().into();
    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let event = LogEvent::from(message.clone()).with_batch_notifier(&batch);
    drop(batch);
    sink.run_events(iter::once(event.into())).await.unwrap();
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
}

#[tokio::test]
async fn splunk_insert_source() {
    let cx = SinkContext::default();

    let mut config = config(TextSerializerConfig::default().into(), vec![]).await;
    config.source = Template::try_from("/var/log/syslog".to_string()).ok();

    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let event = Event::Log(LogEvent::from(message.clone()));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;

    let entry = find_entry(message.as_str()).await;

    assert_eq!(entry[SOURCE_FIELD].as_str(), Some("/var/log/syslog"));
}

#[tokio::test]
async fn splunk_insert_index() {
    let cx = SinkContext::default();

    let mut config = config(TextSerializerConfig::default().into(), vec![]).await;
    config.index = Template::try_from("custom_index".to_string()).ok();
    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let event = LogEvent::from(message.clone());
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;

    let entry = find_entry(message.as_str()).await;

    assert_eq!(entry["index"].as_str().unwrap(), "custom_index");
}

#[tokio::test]
async fn splunk_index_is_interpolated() {
    let cx = SinkContext::default();

    let indexed_fields = vec!["asdf".into()];
    let mut config = config(JsonSerializerConfig::default().into(), indexed_fields).await;
    config.index = Template::try_from("{{ index_name }}".to_string()).ok();

    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let mut event = LogEvent::from(message.clone());
    event.insert("index_name", "custom_index");
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;

    let entry = find_entry(message.as_str()).await;

    let index = entry["index"].as_str().unwrap();
    assert_eq!("custom_index", index);
}

#[tokio::test]
async fn splunk_insert_many() {
    let cx = SinkContext::default();

    let config = config(TextSerializerConfig::default().into(), vec![]).await;
    let (sink, _) = config.build(cx).await.unwrap();

    let (messages, events) = random_lines_with_stream(100, 10, None);
    run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

    assert!(find_entries(messages.as_slice()).await);
}

#[tokio::test]
async fn splunk_custom_fields() {
    let cx = SinkContext::default();

    let indexed_fields = vec!["asdf".into()];
    let config = config(JsonSerializerConfig::default().into(), indexed_fields).await;
    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let mut event = LogEvent::from(message.clone());
    event.insert("asdf", "hello");
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;

    let entry = find_entry(message.as_str()).await;

    assert_eq!(message, entry["message"].as_str().unwrap());
    let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
    assert_eq!("hello", asdf);
}

#[tokio::test]
async fn splunk_hostname() {
    let cx = SinkContext::default();

    let indexed_fields = vec!["asdf".into()];
    let config = config(JsonSerializerConfig::default().into(), indexed_fields).await;
    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let mut event = LogEvent::from(message.clone());
    event.insert("asdf", "hello");
    event.insert("host", "example.com:1234");
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;

    let entry = find_entry(message.as_str()).await;

    assert_eq!(message, entry["message"].as_str().unwrap());
    let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
    assert_eq!("hello", asdf);
    let host = entry["host"].as_array().unwrap()[0].as_str().unwrap();
    assert_eq!("example.com:1234", host);
}

#[tokio::test]
async fn splunk_sourcetype() {
    let cx = SinkContext::default();

    let indexed_fields = vec!["asdf".into()];
    let mut config = config(JsonSerializerConfig::default().into(), indexed_fields).await;
    config.sourcetype = Template::try_from("_json".to_string()).ok();

    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let mut event = LogEvent::from(message.clone());
    event.insert("asdf", "hello");
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;

    let entry = find_entry(message.as_str()).await;

    assert_eq!(message, entry["message"].as_str().unwrap());
    let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
    assert_eq!("hello", asdf);
    let sourcetype = entry["sourcetype"].as_str().unwrap();
    assert_eq!("_json", sourcetype);
}

#[tokio::test]
async fn splunk_configure_hostname() {
    let cx = SinkContext::default();

    let config = HecLogsSinkConfig {
        host_key: OptionalTargetPath::event("roast"),
        ..config(JsonSerializerConfig::default().into(), vec!["asdf".into()]).await
    };

    let (sink, _) = config.build(cx).await.unwrap();

    let message = random_string(100);
    let mut event = LogEvent::from(message.clone());
    event.insert("asdf", "hello");
    event.insert("host", "example.com:1234");
    event.insert("roast", "beef.example.com:1234");
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;

    let entry = find_entry(message.as_str()).await;

    assert_eq!(message, entry["message"].as_str().unwrap());
    let asdf = entry["asdf"].as_array().unwrap()[0].as_str().unwrap();
    assert_eq!("hello", asdf);
    let host = entry["host"].as_array().unwrap()[0].as_str().unwrap();
    assert_eq!("beef.example.com:1234", host);
}

#[tokio::test]
async fn splunk_indexer_acknowledgements() {
    let cx = SinkContext::default();

    let acknowledgements_config = HecClientAcknowledgementsConfig {
        query_interval: NonZeroU8::new(1).unwrap(),
        retry_limit: NonZeroU8::new(5).unwrap(),
        ..Default::default()
    };

    let config = HecLogsSinkConfig {
        default_token: String::from(ACK_TOKEN).into(),
        acknowledgements: acknowledgements_config,
        ..config(JsonSerializerConfig::default().into(), vec!["asdf".into()]).await
    };
    let (sink, _) = config.build(cx).await.unwrap();

    let (tx, mut rx) = BatchNotifier::new_with_receiver();
    let (messages, events) = random_lines_with_stream(100, 10, Some(tx.clone()));
    drop(tx);
    run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

    assert_eq!(rx.try_recv(), Ok(BatchStatus::Delivered));
    assert!(find_entries(messages.as_slice()).await);
}

#[tokio::test]
async fn splunk_indexer_acknowledgements_disabled_on_server() {
    let cx = SinkContext::default();

    let config = config(JsonSerializerConfig::default().into(), vec!["asdf".into()]).await;
    let (sink, _) = config.build(cx).await.unwrap();

    let (tx, mut rx) = BatchNotifier::new_with_receiver();
    let (messages, events) = random_lines_with_stream(100, 10, Some(tx.clone()));
    drop(tx);
    run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;

    // With indexer acknowledgements disabled on the server, events are still
    // acknowledged based on 200 OK
    assert_eq!(rx.try_recv(), Ok(BatchStatus::Delivered));
    assert!(find_entries(messages.as_slice()).await);
}

#[tokio::test]
async fn splunk_auto_extracted_timestamp() {
    // The auto_extract_timestamp setting only works on version 8 and above of splunk.
    // If the splunk version is set to 7, we ignore this test.
    // This environment variable is set by the integration test docker-compose file.
    if std::env::var("CONFIG_VERSION")
        .map(|version| !version.starts_with("7."))
        .unwrap_or(true)
    {
        let cx = SinkContext::default();

        let config = HecLogsSinkConfig {
            auto_extract_timestamp: Some(true),
            timestamp_key: OptionalTargetPath {
                path: Some(OwnedTargetPath::event(lookup::owned_value_path!(
                    "timestamp"
                ))),
            },
            ..config(JsonSerializerConfig::default().into(), vec![]).await
        };

        let (sink, _) = config.build(cx).await.unwrap();

        // With auto_extract_timestamp switched the timestamp comes from the message.
        // Note that as per <https://docs.splunk.com/Documentation/Splunk/latest/Data/Configuretimestamprecognition>
        // by default, the max age of timestamps is 2,000 days old. So we will test with a timestamp that
        // is within that limit.
        let date = Utc::now().with_nanosecond(0).unwrap() - chrono::Duration::days(1999);
        let message = format!("this message is on {}", date.format("%Y-%m-%d %H:%M:%S"));
        let mut event = LogEvent::from(message.as_str());

        event.insert(
            "timestamp",
            Value::from(
                Utc.with_ymd_and_hms(2020, 3, 5, 0, 0, 0)
                    .single()
                    .expect("invalid timestamp"),
            ),
        );

        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;

        let entry = find_entry(&message).await;

        assert_eq!(
            format!("{{\"message\":\"{}\"}}", message),
            entry["_raw"].as_str().unwrap()
        );
        assert_eq!(
            &format!("{}", date.format("%Y-%m-%dT%H:%M:%S%.3f%:z")),
            entry["_time"].as_str().unwrap()
        );
    }
}

#[tokio::test]
async fn splunk_non_auto_extracted_timestamp() {
    // The auto_extract_timestamp setting only works on version 8 and above of splunk.
    // If the splunk version is set to 7, we ignore this test.
    // This environment variable is set by the integration test docker-compose file.
    if std::env::var("CONFIG_VERSION")
        .map(|version| !version.starts_with("7."))
        .unwrap_or(true)
    {
        let cx = SinkContext::default();

        let config = HecLogsSinkConfig {
            auto_extract_timestamp: Some(false),
            timestamp_key: OptionalTargetPath {
                path: Some(OwnedTargetPath::event(lookup::owned_value_path!(
                    "timestamp"
                ))),
            },
            ..config(JsonSerializerConfig::default().into(), vec![]).await
        };

        let (sink, _) = config.build(cx).await.unwrap();
        let date = Utc::now().with_nanosecond(0).unwrap() - chrono::Duration::days(1999);
        let message = format!("this message is on {}", date.format("%Y-%m-%d %H:%M:%S"));
        let mut event = LogEvent::from(message.as_str());

        // With auto_extract_timestamp switched off the timestamp comes from the event timestamp.
        event.insert(
            "timestamp",
            Value::from(
                Utc.with_ymd_and_hms(2020, 3, 5, 0, 0, 0)
                    .single()
                    .expect("invalid timestamp"),
            ),
        );

        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &HTTP_SINK_TAGS).await;

        let entry = find_entry(&message).await;

        assert_eq!(
            format!("{{\"message\":\"{}\"}}", message),
            entry["_raw"].as_str().unwrap()
        );
        assert_eq!(
            "2020-03-05T00:00:00.000+00:00",
            entry["_time"].as_str().unwrap()
        );
    }
}
