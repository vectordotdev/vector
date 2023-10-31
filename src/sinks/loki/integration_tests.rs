use std::convert::TryFrom;
use std::sync::Arc;

use bytes::Bytes;
use chrono::{DateTime, Duration, Utc};
use futures::stream;
use vector_lib::encode_logfmt;
use vector_lib::lookup::{owned_value_path, PathPrefix};
use vector_lib::{
    config::{init_telemetry, LogNamespace, Tags, Telemetry},
    event::{BatchNotifier, BatchStatus, Event, LogEvent},
};
use vrl::value::{kind::Collection, Kind};

use super::config::{LokiConfig, OutOfOrderAction};
use crate::{
    config::{log_schema, SinkConfig},
    event::Value,
    schema,
    sinks::{util::test::load_sink, VectorSink},
    template::Template,
    test_util::{
        components::{
            run_and_assert_data_volume_sink_compliance, run_and_assert_sink_compliance,
            DATA_VOLUME_SINK_TAGS, SINK_TAGS,
        },
        generate_events_with_stream, generate_lines_with_stream, random_lines,
    },
};

fn loki_address() -> String {
    std::env::var("LOKI_ADDRESS").unwrap_or_else(|_| "http://localhost:3100".into())
}

async fn build_sink(codec: &str, remove_timestamp: bool) -> (uuid::Uuid, VectorSink) {
    let stream = uuid::Uuid::new_v4();

    let config = format!(
        r#"
            endpoint = "{}"
            labels = {{test_name = "placeholder"}}
            encoding.codec = "{codec}"
            remove_timestamp = {remove_timestamp}
            tenant_id = "default"
        "#,
        loki_address(),
    );

    let (mut config, cx) = load_sink::<LokiConfig>(&config).unwrap();

    let test_name = config
        .labels
        .get_mut(&Template::try_from("test_name").unwrap())
        .unwrap();
    assert_eq!(test_name.get_ref(), &Bytes::from("placeholder"));

    *test_name = Template::try_from(stream.to_string()).unwrap();

    let (sink, _) = config.build(cx).await.unwrap();

    (stream, sink)
}

async fn build_sink_with_compression(codec: &str, compression: &str) -> (uuid::Uuid, VectorSink) {
    let stream = uuid::Uuid::new_v4();

    let config = format!(
        r#"
            endpoint = "{}"
            labels = {{test_name = "placeholder"}}
            encoding.codec = "{}"
            compression = "{}"
            remove_timestamp = false
            tenant_id = "default"
        "#,
        loki_address(),
        codec,
        compression
    );

    let (mut config, cx) = load_sink::<LokiConfig>(&config).unwrap();

    let test_name = config
        .labels
        .get_mut(&Template::try_from("test_name").unwrap())
        .unwrap();
    assert_eq!(test_name.get_ref(), &Bytes::from("placeholder"));

    *test_name = Template::try_from(stream.to_string()).unwrap();

    let (sink, _) = config.build(cx).await.unwrap();

    (stream, sink)
}

fn line_generator(index: usize) -> String {
    format!("random line {}", index)
}

fn event_generator(index: usize) -> Event {
    Event::Log(LogEvent::from(line_generator(index)))
}

/// A generator that creates a timestamp in the given field.
/// This field is given the meaning of `timestamp` to ensure that in the Vector namespace
/// we use this field as a timestamp.
fn namespaced_timestamp_generator(
    timestamp_field: String,
    timestamp: chrono::DateTime<Utc>,
) -> impl Fn(usize) -> Event {
    move |index: usize| {
        let mut log = LogEvent::default();
        log.insert("message", line_generator(index));
        log.insert(timestamp_field.as_str(), Value::from(timestamp));

        // We need vector metadata for it to pick up that it is in the Vector namespace.
        LogNamespace::Vector.insert_standard_vector_source_metadata(&mut log, "loki", Utc::now());

        let schema = schema::Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Vector],
        )
        // Add a field that means the timestamp.
        .with_event_field(
            &owned_value_path!(&timestamp_field),
            Kind::timestamp(),
            Some("timestamp"),
        );

        let mut event = Event::from(log);
        event
            .metadata_mut()
            .set_schema_definition(&Arc::new(schema));

        event
    }
}

#[tokio::test]
async fn text() {
    let (stream, sink) = build_sink("text", false).await;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (lines, events) = generate_lines_with_stream(line_generator, 10, Some(batch));
    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
    assert_eq!(lines.len(), outputs.len());
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(output, &lines[i]);
    }
}

#[tokio::test]
async fn data_volume_tags() {
    init_telemetry(
        Telemetry {
            tags: Tags {
                emit_service: true,
                emit_source: true,
            },
        },
        true,
    );

    let (stream, sink) = build_sink("text", false).await;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (lines, events) = generate_lines_with_stream(line_generator, 10, Some(batch));
    run_and_assert_data_volume_sink_compliance(sink, events, &DATA_VOLUME_SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
    assert_eq!(lines.len(), outputs.len());
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(output, &lines[i]);
    }
}

#[tokio::test]
async fn namespaced_timestamp() {
    let (stream, sink) = build_sink("json", true).await;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let timestamp = Utc::now() - Duration::minutes(5);
    let (lines, events) = generate_events_with_stream(
        namespaced_timestamp_generator("norknork".to_string(), timestamp),
        10,
        Some(batch),
    );
    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (timestamps, outputs) = fetch_stream(stream.to_string(), "default").await;
    assert_eq!(lines.len(), outputs.len());
    for (i, output) in outputs.iter().enumerate() {
        let output = serde_json::from_str::<serde_json::Value>(output).unwrap();
        let output = output.as_object().unwrap();

        // The sink should have removed the `norknork` field because we have configured
        // it to remove the timestamp field.
        assert!(!output.contains_key("norknork"));

        // The timestamp of the event needs to be the timestamp set in the `norknork`
        // field since that was given the meaning of `timestamp`.
        assert_eq!(
            timestamp
                .timestamp_nanos_opt()
                .expect("Timestamp out of range"),
            timestamps[i]
        );
    }
}

#[tokio::test]
async fn text_with_none_compression() {
    let (stream, sink) = build_sink_with_compression("text", "none").await;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (lines, events) = generate_lines_with_stream(line_generator, 10, Some(batch));
    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
    assert_eq!(lines.len(), outputs.len());
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(output, &lines[i]);
    }
}

#[tokio::test]
async fn text_with_gzip_compression() {
    let (stream, sink) = build_sink_with_compression("text", "gzip").await;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (lines, events) = generate_lines_with_stream(line_generator, 10, Some(batch));
    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
    assert_eq!(lines.len(), outputs.len());
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(output, &lines[i]);
    }
}

#[tokio::test]
async fn json() {
    let (stream, sink) = build_sink("json", false).await;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (lines, events) = generate_events_with_stream(event_generator, 10, Some(batch));
    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
    assert_eq!(lines.len(), outputs.len());
    for (i, output) in outputs.iter().enumerate() {
        let expected_json = serde_json::to_string(&lines[i].as_log()).unwrap();
        assert_eq!(output, &expected_json);
    }
}

// https://github.com/vectordotdev/vector/issues/7815
#[tokio::test]
async fn json_nested_fields() {
    let (stream, sink) = build_sink("json", false).await;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let generator = |idx| {
        let mut event = event_generator(idx);
        let log = event.as_mut_log();
        log.insert("foo.bar", "baz");
        event
    };
    let (lines, events) = generate_events_with_stream(generator, 10, Some(batch));
    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
    assert_eq!(lines.len(), outputs.len());
    for (i, output) in outputs.iter().enumerate() {
        let expected_json = serde_json::to_string(&lines[i].as_log()).unwrap();
        assert_eq!(output, &expected_json);
    }
}

#[tokio::test]
async fn logfmt() {
    let (stream, sink) = build_sink("logfmt", false).await;

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (lines, events) = generate_events_with_stream(event_generator, 10, Some(batch));
    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs) = fetch_stream(stream.to_string(), "default").await;
    assert_eq!(lines.len(), outputs.len());
    for (i, output) in outputs.iter().enumerate() {
        let expected_logfmt = encode_logfmt::encode_value(lines[i].as_log().value()).unwrap();
        assert_eq!(output, &expected_logfmt);
    }
}

#[tokio::test]
async fn many_streams() {
    let stream1 = uuid::Uuid::new_v4();
    let stream2 = uuid::Uuid::new_v4();

    let config = format!("endpoint = \"{}\"", loki_address())
        + r#"
            labels = {test_name = "{{ stream_id }}"}
            encoding.codec = "text"
            tenant_id = "default"
        "#;
    let (config, cx) = load_sink::<LokiConfig>(config.as_str()).unwrap();

    let (sink, _) = config.build(cx).await.unwrap();

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let generator = move |idx| {
        let mut event = event_generator(idx);
        if idx < 10 {
            let log = event.as_mut_log();
            if idx % 2 == 0 {
                log.insert("stream_id", stream1.to_string());
            } else {
                log.insert("stream_id", stream2.to_string());
            }
        }
        event
    };
    let (lines, events) = generate_events_with_stream(generator, 10, Some(batch));

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs1) = fetch_stream(stream1.to_string(), "default").await;
    let (_, outputs2) = fetch_stream(stream2.to_string(), "default").await;

    assert_eq!(outputs1.len() + outputs2.len(), lines.len());

    for (i, output) in outputs1.iter().enumerate() {
        let index = (i % 5) * 2;
        let message = lines[index]
            .as_log()
            .get(log_schema().message_key_target_path().unwrap())
            .unwrap()
            .to_string_lossy();
        assert_eq!(output, &message);
    }

    for (i, output) in outputs2.iter().enumerate() {
        let index = ((i % 5) * 2) + 1;
        let message = lines[index]
            .as_log()
            .get(log_schema().message_key_target_path().unwrap())
            .unwrap()
            .to_string_lossy();
        assert_eq!(output, &message);
    }
}

#[tokio::test]
async fn interpolate_stream_key() {
    let stream = uuid::Uuid::new_v4();

    let config = format!("endpoint = \"{}\"", loki_address())
        + r#"
            labels = {"{{ stream_key }}" = "placeholder"}
            encoding.codec = "text"
            tenant_id = "default"
        "#;
    let (mut config, cx) = load_sink::<LokiConfig>(config.as_str()).unwrap();
    config.labels.insert(
        Template::try_from("{{ stream_key }}").unwrap(),
        Template::try_from(stream.to_string()).unwrap(),
    );

    let (sink, _) = config.build(cx).await.unwrap();

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let generator = |idx| {
        let mut event = event_generator(idx);
        if idx < 10 {
            event.as_mut_log().insert("stream_key", "test_name");
        }
        event
    };
    let (lines, events) = generate_events_with_stream(generator, 10, Some(batch));

    run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs) = fetch_stream(stream.to_string(), "default").await;

    assert_eq!(outputs.len(), lines.len());

    for (i, output) in outputs.iter().enumerate() {
        let message = lines[i]
            .as_log()
            .get(log_schema().message_key_target_path().unwrap())
            .unwrap()
            .to_string_lossy();
        assert_eq!(output, &message);
    }
}

#[tokio::test]
async fn many_tenants() {
    let stream = uuid::Uuid::new_v4();

    let config = format!("endpoint = \"{}\"", loki_address())
        + r#"
            labels = {test_name = "placeholder"}
            encoding.codec = "text"
            tenant_id = "{{ tenant_id }}"
        "#;
    let (mut config, cx) = load_sink::<LokiConfig>(config.as_str()).unwrap();

    let test_name = config
        .labels
        .get_mut(&Template::try_from("test_name").unwrap())
        .unwrap();
    assert_eq!(test_name.get_ref(), &Bytes::from("placeholder"));

    *test_name = Template::try_from(stream.to_string()).unwrap();

    let (sink, _) = config.build(cx).await.unwrap();

    let lines = random_lines(100).take(10).collect::<Vec<_>>();

    let mut events = lines
        .clone()
        .into_iter()
        .map(|e| Event::Log(LogEvent::from(e)))
        .collect::<Vec<_>>();

    for (i, event) in events.iter_mut().enumerate() {
        let log = event.as_mut_log();

        log.insert("tenant_id", if i % 2 == 0 { "tenant1" } else { "tenant2" });
    }

    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (_, outputs1) = fetch_stream(stream.to_string(), "tenant1").await;
    let (_, outputs2) = fetch_stream(stream.to_string(), "tenant2").await;

    assert_eq!(outputs1.len() + outputs2.len(), lines.len());

    for (i, output) in outputs1.iter().enumerate() {
        let index = (i % 5) * 2;
        assert_eq!(output, &lines[index]);
    }

    for (i, output) in outputs2.iter().enumerate() {
        let index = ((i % 5) * 2) + 1;
        assert_eq!(output, &lines[index]);
    }
}

#[tokio::test]
async fn out_of_order_drop() {
    let batch_size = 5;
    let lines = random_lines(100).take(10).collect::<Vec<_>>();
    let mut events = lines
        .clone()
        .into_iter()
        .map(|e| Event::Log(LogEvent::from(e)))
        .collect::<Vec<_>>();

    let base = Utc::now() - Duration::seconds(20);
    for (i, event) in events.iter_mut().enumerate() {
        let log = event.as_mut_log();
        log.insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            base + Duration::seconds(i as i64),
        );
    }
    // first event of the second batch is out-of-order.
    events[batch_size].as_mut_log().insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        base,
    );

    let mut expected = events.clone();
    expected.remove(batch_size);

    test_out_of_order_events(OutOfOrderAction::Drop, batch_size, events, expected).await;
}

#[tokio::test]
async fn out_of_order_accept() {
    let batch_size = 5;
    let lines = random_lines(100).take(10).collect::<Vec<_>>();
    let mut events = lines
        .clone()
        .into_iter()
        .map(|e| Event::Log(LogEvent::from(e)))
        .collect::<Vec<_>>();

    let base = Utc::now() - Duration::seconds(20);
    for (i, event) in events.iter_mut().enumerate() {
        let log = event.as_mut_log();
        log.insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            base + Duration::seconds(i as i64),
        );
    }
    // first event of the second batch is out-of-order.
    events[batch_size].as_mut_log().insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        base - Duration::seconds(1),
    );

    // move out-of-order event to where it will appear in results
    let mut expected = events.clone();
    let reordered_event = expected.remove(batch_size);
    let expected = vec![reordered_event].into_iter().chain(expected).collect();

    test_out_of_order_events(OutOfOrderAction::Accept, batch_size, events, expected).await;
}

#[tokio::test]
async fn out_of_order_rewrite() {
    let batch_size = 5;
    let lines = random_lines(100).take(10).collect::<Vec<_>>();
    let mut events = lines
        .clone()
        .into_iter()
        .map(|e| Event::Log(LogEvent::from(e)))
        .collect::<Vec<_>>();

    let base = Utc::now() - Duration::seconds(20);
    for (i, event) in events.iter_mut().enumerate() {
        let log = event.as_mut_log();
        log.insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            base + Duration::seconds(i as i64),
        );
    }
    // first event of the second batch is out-of-order.
    events[batch_size].as_mut_log().insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        base,
    );

    let mut expected = events.clone();
    let time = get_timestamp(&expected[batch_size - 1]);
    // timestamp is rewritten with latest timestamp of the first batch
    expected[batch_size].as_mut_log().insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        time,
    );

    test_out_of_order_events(
        OutOfOrderAction::RewriteTimestamp,
        batch_size,
        events,
        expected,
    )
    .await;
}

#[tokio::test]
async fn out_of_order_per_partition() {
    let batch_size = 2;
    let big_lines = random_lines(1_000_000).take(2);
    let small_lines = random_lines(1).take(20);
    let mut events = big_lines
        .into_iter()
        .chain(small_lines)
        .map(|e| Event::Log(LogEvent::from(e)))
        .collect::<Vec<_>>();

    let base = Utc::now() - Duration::seconds(30);
    for (i, event) in events.iter_mut().enumerate() {
        let log = event.as_mut_log();
        log.insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            base + Duration::seconds(i as i64),
        );
    }

    // So, since all of the events are of the same partition, and if there is concurrency,
    // then if ordering inside partitions isn't upheld, the big line events will take longer
    // time to flush than small line events so loki will receive smaller ones before large
    // ones hence out of order events.
    test_out_of_order_events(OutOfOrderAction::Drop, batch_size, events.clone(), events).await;
}

async fn test_out_of_order_events(
    action: OutOfOrderAction,
    batch_size: usize,
    events: Vec<Event>,
    expected: Vec<Event>,
) {
    crate::test_util::trace_init();
    let stream = uuid::Uuid::new_v4();

    let config = format!("endpoint = \"{}\"", loki_address())
        + r#"
            labels = {test_name = "placeholder"}
            encoding.codec = "text"
            tenant_id = "default"
        "#;
    let (mut config, cx) = load_sink::<LokiConfig>(config.as_str()).unwrap();
    config.out_of_order_action = action;
    config.labels.insert(
        Template::try_from("test_name").unwrap(),
        Template::try_from(stream.to_string()).unwrap(),
    );
    config.batch.max_events = Some(batch_size);
    config.batch.max_bytes = Some(4_000_000);

    let (sink, _) = config.build(cx).await.unwrap();
    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    tokio::time::sleep(tokio::time::Duration::new(1, 0)).await;

    let (timestamps, outputs) = fetch_stream(stream.to_string(), "default").await;
    assert_eq!(expected.len(), outputs.len());
    assert_eq!(expected.len(), timestamps.len());
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(
            &expected[i]
                .as_log()
                .get(log_schema().message_key_target_path().unwrap())
                .unwrap()
                .to_string_lossy(),
            output,
        )
    }
    for (i, ts) in timestamps.iter().enumerate() {
        assert_eq!(
            get_timestamp(&expected[i])
                .timestamp_nanos_opt()
                .expect("Timestamp out of range"),
            *ts
        );
    }
}

fn get_timestamp(event: &Event) -> DateTime<Utc> {
    *event
        .as_log()
        .get((PathPrefix::Event, log_schema().timestamp_key().unwrap()))
        .unwrap()
        .as_timestamp()
        .unwrap()
}

async fn fetch_stream(stream: String, tenant: &str) -> (Vec<i64>, Vec<String>) {
    let query = format!("%7Btest_name%3D\"{}\"%7D", stream);
    let query = format!(
        "{}/loki/api/v1/query_range?query={}&direction=forward",
        loki_address(),
        query
    );

    let res = reqwest::Client::new()
        .get(&query)
        .header("X-Scope-OrgID", tenant)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);

    // The response type follows this api https://github.com/grafana/loki/blob/master/docs/api.md#get-lokiapiv1query_range
    // where the result type is `streams`.
    let data = res.json::<serde_json::Value>().await.unwrap();

    // TODO: clean this up or explain it via docs
    let results = data
        .get("data")
        .unwrap()
        .get("result")
        .unwrap()
        .as_array()
        .unwrap();

    let values = results[0].get("values").unwrap().as_array().unwrap();

    // the array looks like: [ts, line].
    let timestamps = values
        .iter()
        .map(|v| v[0].as_str().unwrap().parse().unwrap())
        .collect();
    let lines = values
        .iter()
        .map(|v| v[1].as_str().unwrap().to_string())
        .collect();
    (timestamps, lines)
}
