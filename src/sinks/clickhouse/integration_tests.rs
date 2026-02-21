use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use futures::{
    future::{ok, ready},
    stream,
};
use http::StatusCode;
use ordered_float::NotNan;
use serde::Deserialize;
use serde_json::Value;
use tokio::time::{Duration, timeout};
use vector_lib::{
    codecs::encoding::{ArrowStreamSerializerConfig, BatchSerializerConfig},
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent},
    lookup::PathPrefix,
};
use warp::Filter;

use crate::{
    codecs::{TimestampFormat, Transformer},
    config::{SinkConfig, SinkContext, log_schema},
    sinks::{
        clickhouse::config::ClickhouseConfig,
        util::{BatchConfig, Compression, TowerRequestConfig},
    },
    test_util::{
        components::{SINK_TAGS, init_test, run_and_assert_sink_compliance},
        random_table_name, trace_init,
    },
};
use vector_lib::metrics::Controller;

fn clickhouse_address() -> String {
    std::env::var("CLICKHOUSE_ADDRESS").unwrap_or_else(|_| "http://localhost:8123".into())
}

#[tokio::test]
async fn insert_events() {
    trace_init();

    let table = random_table_name();
    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: table.clone().try_into().unwrap(),
        compression: Compression::None,
        batch,
        request: TowerRequestConfig {
            retry_attempts: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    let client = ClickhouseClient::new(host);
    client
        .create_table(
            &table,
            "host String, timestamp String, message String, items Array(String)",
        )
        .await;

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (mut input_event, mut receiver) = make_event();
    input_event
        .as_mut_log()
        .insert("items", vec!["item1", "item2"]);

    run_and_assert_sink_compliance(sink, stream::once(ready(input_event.clone())), &SINK_TAGS)
        .await;

    let output = client.select_all(&table).await;
    assert_eq!(1, output.rows);

    let expected = serde_json::to_value(input_event.into_log()).unwrap();
    assert_eq!(expected, output.data[0]);

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
}

#[tokio::test]
async fn skip_unknown_fields() {
    trace_init();

    let table = random_table_name();
    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: table.clone().try_into().unwrap(),
        skip_unknown_fields: Some(true),
        compression: Compression::None,
        batch,
        request: TowerRequestConfig {
            retry_attempts: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    let client = ClickhouseClient::new(host);
    client
        .create_table(&table, "host String, timestamp String, message String")
        .await;

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (mut input_event, mut receiver) = make_event();
    input_event.as_mut_log().insert("unknown", "mysteries");

    run_and_assert_sink_compliance(sink, stream::once(ready(input_event.clone())), &SINK_TAGS)
        .await;

    let output = client.select_all(&table).await;
    assert_eq!(1, output.rows);

    input_event.as_mut_log().remove("unknown");
    let expected = serde_json::to_value(input_event.into_log()).unwrap();
    assert_eq!(expected, output.data[0]);

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
}

#[tokio::test]
async fn insert_events_unix_timestamps() {
    trace_init();

    let table = random_table_name();
    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: table.clone().try_into().unwrap(),
        compression: Compression::None,
        encoding: Transformer::new(None, None, Some(TimestampFormat::Unix)).unwrap(),
        batch,
        request: TowerRequestConfig {
            retry_attempts: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    let client = ClickhouseClient::new(host);
    client
        .create_table(
            &table,
            "host String, timestamp DateTime('UTC'), message String",
        )
        .await;

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (mut input_event, _receiver) = make_event();

    run_and_assert_sink_compliance(sink, stream::once(ready(input_event.clone())), &SINK_TAGS)
        .await;

    let output = client.select_all(&table).await;
    assert_eq!(1, output.rows);

    let exp_event = input_event.as_mut_log();
    exp_event.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        format!(
            "{}",
            exp_event
                .get_timestamp()
                .unwrap()
                .as_timestamp()
                .unwrap()
                .format("%Y-%m-%d %H:%M:%S")
        ),
    );

    let expected = serde_json::to_value(exp_event).unwrap();
    assert_eq!(expected, output.data[0]);
}

#[tokio::test]
async fn insert_events_unix_timestamps_toml_config() {
    trace_init();

    let table = random_table_name();
    let host = clickhouse_address();

    let config: ClickhouseConfig = toml::from_str(&format!(
        r#"
host = "{host}"
table = "{table}"
compression = "none"
[request]
retry_attempts = 1
[batch]
max_events = 1
[encoding]
timestamp_format = "unix""#
    ))
    .unwrap();

    let client = ClickhouseClient::new(host);
    client
        .create_table(
            &table,
            "host String, timestamp DateTime('UTC'), message String",
        )
        .await;

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (mut input_event, _receiver) = make_event();

    run_and_assert_sink_compliance(sink, stream::once(ready(input_event.clone())), &SINK_TAGS)
        .await;

    let output = client.select_all(&table).await;
    assert_eq!(1, output.rows);

    let exp_event = input_event.as_mut_log();
    exp_event.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        format!(
            "{}",
            exp_event
                .get_timestamp()
                .unwrap()
                .as_timestamp()
                .unwrap()
                .format("%Y-%m-%d %H:%M:%S")
        ),
    );

    let expected = serde_json::to_value(exp_event).unwrap();
    assert_eq!(expected, output.data[0]);
}

#[tokio::test]
async fn no_retry_on_incorrect_data() {
    trace_init();

    let table = random_table_name();
    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: table.clone().try_into().unwrap(),
        compression: Compression::None,
        batch,
        ..Default::default()
    };

    let client = ClickhouseClient::new(host);
    // The event contains a message field, but it's of type String, which will cause
    // the request to fail.
    client
        .create_table(&table, "host String, timestamp String, message Int32")
        .await;

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (input_event, mut receiver) = make_event();

    // Retries should go on forever, so if we are retrying incorrectly
    // this timeout should trigger.
    timeout(Duration::from_secs(5), sink.run_events(vec![input_event]))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
}

#[tokio::test]
async fn no_retry_on_incorrect_data_warp() {
    trace_init();

    let visited = Arc::new(AtomicBool::new(false));
    let routes = warp::any().and_then(move || {
        assert!(!visited.load(Ordering::SeqCst), "Should not retry request.");
        visited.store(true, Ordering::SeqCst);

        ok::<_, Infallible>(warp::reply::with_status(
            "Code: 117",
            StatusCode::INTERNAL_SERVER_ERROR,
        ))
    });
    let server = warp::serve(routes).bind("0.0.0.0:8124".parse::<SocketAddr>().unwrap());
    tokio::spawn(server);

    let host = String::from("http://localhost:8124");

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: random_table_name().try_into().unwrap(),
        batch,
        ..Default::default()
    };
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (input_event, mut receiver) = make_event();

    // Retries should go on forever, so if we are retrying incorrectly
    // this timeout should trigger.
    timeout(Duration::from_secs(5), sink.run_events(vec![input_event]))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Errored));
}

#[tokio::test]
async fn templated_table() {
    trace_init();

    let n_tables = 2;
    let table_events: Vec<(String, Event, BatchStatusReceiver)> = (0..n_tables)
        .map(|_| {
            let table = random_table_name();
            let (mut event, receiver) = make_event();
            event.as_mut_log().insert("table", table.as_str());
            (table, event, receiver)
        })
        .collect();

    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: "{{ .table }}".try_into().unwrap(),
        batch,
        ..Default::default()
    };

    let client = ClickhouseClient::new(host);
    for (table, _, _) in &table_events {
        client
            .create_table(
                table,
                "host String, timestamp String, message String, table String",
            )
            .await;
    }

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let events: Vec<Event> = table_events
        .iter()
        .map(|(_, event, _)| event.clone())
        .collect();
    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    for (table, event, mut receiver) in table_events {
        let output = client.select_all(&table).await;
        assert_eq!(1, output.rows, "table {table} should have 1 row");

        let expected = serde_json::to_value(event.into_log()).unwrap();
        assert_eq!(
            expected, output.data[0],
            "table \"{table}\"'s one row should have the correct data"
        );

        assert_eq!(
            receiver.try_recv(),
            Ok(BatchStatus::Delivered),
            "table \"{table}\"'s event should have been delivered"
        );
    }
}

fn make_event() -> (Event, BatchStatusReceiver) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let mut event = LogEvent::from("raw log line").with_batch_notifier(&batch);
    event.insert("host", "example.com");
    (event.into(), receiver)
}

struct ClickhouseClient {
    host: String,
    client: reqwest::Client,
}

impl ClickhouseClient {
    fn new(host: String) -> Self {
        ClickhouseClient {
            host,
            client: reqwest::Client::new(),
        }
    }

    async fn create_table(&self, table: &str, schema: &str) {
        let response = self
            .client
            .post(&self.host)
            .body(format!(
                "CREATE TABLE {table}
                    ({schema})
                    ENGINE = MergeTree()
                    ORDER BY (host, timestamp);"
            ))
            .send()
            .await
            .unwrap();

        if !response.status().is_success() {
            panic!("create table failed: {}", response.text().await.unwrap())
        }
    }

    async fn select_all(&self, table: &str) -> QueryResponse {
        let response = self
            .client
            .post(&self.host)
            .body(format!("SELECT * FROM {table} FORMAT JSON"))
            .send()
            .await
            .unwrap();

        if !response.status().is_success() {
            panic!("select all failed: {}", response.text().await.unwrap())
        } else {
            let text = response.text().await.unwrap();
            match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(_) => panic!("json failed: {text:?}"),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // deserialize all fields
struct QueryResponse {
    data: Vec<Value>,
    meta: Vec<Value>,
    rows: usize,
    statistics: Stats,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // deserialize all fields
struct Stats {
    bytes_read: usize,
    elapsed: f64,
    rows_read: usize,
}

#[tokio::test]
async fn insert_events_arrow_format() {
    trace_init();

    let table = random_table_name();
    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(5);

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: table.clone().try_into().unwrap(),
        compression: Compression::None,
        format: crate::sinks::clickhouse::config::Format::ArrowStream,
        batch_encoding: Some(BatchSerializerConfig::ArrowStream(Default::default())),
        batch,
        request: TowerRequestConfig {
            retry_attempts: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    let client = ClickhouseClient::new(host.clone());

    client
        .create_table(
            &table,
            "host String, timestamp DateTime64(3), message String, count Int64",
        )
        .await;

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let mut events: Vec<Event> = Vec::new();
    for i in 0..5 {
        let mut event = LogEvent::from(format!("log message {}", i));
        event.insert("host", format!("host{}.example.com", i));
        event.insert("count", i as i64);
        events.push(event.into());
    }

    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    let output = client.select_all(&table).await;
    assert_eq!(5, output.rows);

    // Verify fields exist and are correctly typed
    for row in output.data.iter() {
        assert!(row.get("host").and_then(|v| v.as_str()).is_some());
        assert!(row.get("message").and_then(|v| v.as_str()).is_some());
        assert!(
            row.get("count")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
                .is_some()
        );
    }
}

#[tokio::test]
async fn insert_events_arrow_with_schema_fetching() {
    trace_init();

    let table = random_table_name();
    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(3);

    let client = ClickhouseClient::new(host.clone());

    // Create table with specific typed columns including various data types
    // Include standard Vector log fields: host, timestamp, message
    client
        .create_table(
            &table,
            "host String, timestamp DateTime64(3), message String, id Int64, name String, score Float64, active Bool",
        )
        .await;

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: table.clone().try_into().unwrap(),
        compression: Compression::None,
        format: crate::sinks::clickhouse::config::Format::ArrowStream,
        batch_encoding: Some(BatchSerializerConfig::ArrowStream(Default::default())),
        batch,
        request: TowerRequestConfig {
            retry_attempts: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    // Building the sink should fetch the schema from ClickHouse
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    // Create events with various types that should match the schema
    let mut events: Vec<Event> = Vec::new();
    for i in 0..3 {
        let mut event = LogEvent::from(format!("Test message {}", i));
        event.insert("host", format!("host{}.example.com", i));
        event.insert("id", i as i64);
        event.insert("name", format!("user_{}", i));
        event.insert("score", 95.5 + i as f64);
        event.insert("active", i % 2 == 0);
        events.push(event.into());
    }

    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    let output = client.select_all(&table).await;
    assert_eq!(3, output.rows);

    // Verify all fields exist and have the correct types
    for row in output.data.iter() {
        // Check standard Vector fields exist
        assert!(row.get("host").and_then(|v| v.as_str()).is_some());
        assert!(row.get("message").and_then(|v| v.as_str()).is_some());
        assert!(row.get("timestamp").is_some());

        // Check custom fields have correct types
        assert!(
            row.get("id")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
                .is_some()
        );
        assert!(row.get("name").and_then(|v| v.as_str()).is_some());
        assert!(row.get("score").and_then(|v| v.as_f64()).is_some());
        assert!(row.get("active").and_then(|v| v.as_bool()).is_some());
    }
}

#[tokio::test]
async fn test_complex_types() {
    trace_init();

    let table = random_table_name();
    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(3);

    let arrow_config = ArrowStreamSerializerConfig {
        allow_nullable_fields: true,
        ..Default::default()
    };

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: table.clone().try_into().unwrap(),
        compression: Compression::None,
        format: crate::sinks::clickhouse::config::Format::ArrowStream,
        batch_encoding: Some(BatchSerializerConfig::ArrowStream(arrow_config)),
        batch,
        request: TowerRequestConfig {
            retry_attempts: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    let client = ClickhouseClient::new(host);

    // Comprehensive schema with all complex types
    client
        .create_table(
            &table,
            "host String, timestamp DateTime64(3), message String, \
             nested_int_array Array(Array(Int32)), \
             nested_string_array Array(Array(String)), \
             array_map Map(String, Array(String)), \
             int_array_map Map(String, Array(Int64)), \
             tuple_with_array Tuple(String, Array(Int32)), \
             tuple_with_map Tuple(String, Map(String, Float64)), \
             tuple_with_nested Tuple(String, Array(Int32), Map(String, Float64)), \
             locations Array(Tuple(String, Float64, Float64)), \
             tags_history Array(Map(String, String)), \
             metrics_history Array(Map(String, Int32)), \
             request_headers Map(String, String), \
             response_metrics Tuple(Int32, Int64, Float64), \
             tags Array(String), \
             user_properties Map(String, Array(String)), \
             array_with_nulls Array(Nullable(Int32)), \
             array_with_named_tuple Array(Tuple(category String, tag String))",
        )
        .await;

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let mut events: Vec<Event> = Vec::new();

    // Event 1: Comprehensive test with all complex types
    let mut event1 = LogEvent::from("Comprehensive complex types test");
    event1.insert("host", "host1.example.com");

    // Nested arrays
    event1.insert(
        "nested_int_array",
        vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Array(vec![
                vector_lib::event::Value::Integer(1),
                vector_lib::event::Value::Integer(2),
            ]),
            vector_lib::event::Value::Array(vec![
                vector_lib::event::Value::Integer(3),
                vector_lib::event::Value::Integer(4),
            ]),
        ]),
    );
    event1.insert(
        "nested_string_array",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Bytes("a".into()),
            vector_lib::event::Value::Bytes("b".into()),
        ])]),
    );

    // Maps with arrays
    let mut array_map = vector_lib::event::ObjectMap::new();
    array_map.insert(
        "fruits".into(),
        vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Bytes("apple".into()),
            vector_lib::event::Value::Bytes("banana".into()),
        ]),
    );
    event1.insert("array_map", vector_lib::event::Value::Object(array_map));

    let mut int_array_map = vector_lib::event::ObjectMap::new();
    int_array_map.insert(
        "scores".into(),
        vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Integer(95),
            vector_lib::event::Value::Integer(87),
        ]),
    );
    event1.insert(
        "int_array_map",
        vector_lib::event::Value::Object(int_array_map),
    );

    // Tuples with complex types
    let mut tuple_with_array = vector_lib::event::ObjectMap::new();
    tuple_with_array.insert(
        "f0".into(),
        vector_lib::event::Value::Bytes("numbers".into()),
    );
    tuple_with_array.insert(
        "f1".into(),
        vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Integer(10),
            vector_lib::event::Value::Integer(20),
        ]),
    );
    event1.insert(
        "tuple_with_array",
        vector_lib::event::Value::Object(tuple_with_array),
    );

    let mut inner_map = vector_lib::event::ObjectMap::new();
    inner_map.insert(
        "temp".into(),
        vector_lib::event::Value::Float(NotNan::new(22.5).unwrap()),
    );
    let mut tuple_with_map = vector_lib::event::ObjectMap::new();
    tuple_with_map.insert(
        "f0".into(),
        vector_lib::event::Value::Bytes("metrics".into()),
    );
    tuple_with_map.insert("f1".into(), vector_lib::event::Value::Object(inner_map));
    event1.insert(
        "tuple_with_map",
        vector_lib::event::Value::Object(tuple_with_map),
    );

    let mut inner_map2 = vector_lib::event::ObjectMap::new();
    inner_map2.insert(
        "avg".into(),
        vector_lib::event::Value::Float(NotNan::new(95.5).unwrap()),
    );
    let mut tuple_complex = vector_lib::event::ObjectMap::new();
    tuple_complex.insert(
        "f0".into(),
        vector_lib::event::Value::Bytes("results".into()),
    );
    tuple_complex.insert(
        "f1".into(),
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Integer(95)]),
    );
    tuple_complex.insert("f2".into(), vector_lib::event::Value::Object(inner_map2));
    event1.insert(
        "tuple_with_nested",
        vector_lib::event::Value::Object(tuple_complex),
    );

    // Array of tuples
    let mut loc1 = vector_lib::event::ObjectMap::new();
    loc1.insert(
        "f0".into(),
        vector_lib::event::Value::Bytes("San Francisco".into()),
    );
    loc1.insert(
        "f1".into(),
        vector_lib::event::Value::Float(NotNan::new(37.7749).unwrap()),
    );
    loc1.insert(
        "f2".into(),
        vector_lib::event::Value::Float(NotNan::new(-122.4194).unwrap()),
    );
    event1.insert(
        "locations",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Object(loc1)]),
    );

    // Array of maps
    let mut tags1 = vector_lib::event::ObjectMap::new();
    tags1.insert("env".into(), vector_lib::event::Value::Bytes("prod".into()));
    event1.insert(
        "tags_history",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Object(tags1)]),
    );

    let mut metrics1 = vector_lib::event::ObjectMap::new();
    metrics1.insert("cpu".into(), vector_lib::event::Value::Integer(45));
    event1.insert(
        "metrics_history",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Object(metrics1)]),
    );

    // Structured log data
    let mut headers = vector_lib::event::ObjectMap::new();
    headers.insert(
        "user-agent".into(),
        vector_lib::event::Value::Bytes("Mozilla/5.0".into()),
    );
    event1.insert("request_headers", vector_lib::event::Value::Object(headers));

    let mut metrics = vector_lib::event::ObjectMap::new();
    metrics.insert("f0".into(), vector_lib::event::Value::Integer(200));
    metrics.insert("f1".into(), vector_lib::event::Value::Integer(1234));
    metrics.insert(
        "f2".into(),
        vector_lib::event::Value::Float(NotNan::new(0.145).unwrap()),
    );
    event1.insert(
        "response_metrics",
        vector_lib::event::Value::Object(metrics),
    );

    event1.insert(
        "tags",
        vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Bytes("api".into()),
            vector_lib::event::Value::Bytes("v2".into()),
        ]),
    );

    let mut user_props = vector_lib::event::ObjectMap::new();
    user_props.insert(
        "roles".into(),
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Bytes("admin".into())]),
    );
    event1.insert(
        "user_properties",
        vector_lib::event::Value::Object(user_props),
    );

    // Nullable array
    event1.insert(
        "array_with_nulls",
        vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Integer(100),
            vector_lib::event::Value::Integer(200),
        ]),
    );

    // Named tuple array - tests that named fields work correctly
    let mut named_tuple1 = vector_lib::event::ObjectMap::new();
    named_tuple1.insert(
        "category".into(),
        vector_lib::event::Value::Bytes("priority".into()),
    );
    named_tuple1.insert("tag".into(), vector_lib::event::Value::Bytes("high".into()));

    let mut named_tuple2 = vector_lib::event::ObjectMap::new();
    named_tuple2.insert(
        "category".into(),
        vector_lib::event::Value::Bytes("environment".into()),
    );
    named_tuple2.insert(
        "tag".into(),
        vector_lib::event::Value::Bytes("production".into()),
    );

    event1.insert(
        "array_with_named_tuple",
        vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Object(named_tuple1),
            vector_lib::event::Value::Object(named_tuple2),
        ]),
    );

    events.push(event1.into());

    // Event 2: Empty and edge cases
    let mut event2 = LogEvent::from("Test empty collections");
    event2.insert("host", "host2.example.com");
    event2.insert("nested_int_array", vector_lib::event::Value::Array(vec![]));
    event2.insert(
        "nested_string_array",
        vector_lib::event::Value::Array(vec![]),
    );

    let empty_map = vector_lib::event::ObjectMap::new();
    event2.insert(
        "array_map",
        vector_lib::event::Value::Object(empty_map.clone()),
    );
    event2.insert(
        "int_array_map",
        vector_lib::event::Value::Object(empty_map.clone()),
    );

    let mut empty_tuple = vector_lib::event::ObjectMap::new();
    empty_tuple.insert("f0".into(), vector_lib::event::Value::Bytes("empty".into()));
    empty_tuple.insert("f1".into(), vector_lib::event::Value::Array(vec![]));
    event2.insert(
        "tuple_with_array",
        vector_lib::event::Value::Object(empty_tuple),
    );

    let mut empty_tuple_map = vector_lib::event::ObjectMap::new();
    empty_tuple_map.insert("f0".into(), vector_lib::event::Value::Bytes("empty".into()));
    empty_tuple_map.insert(
        "f1".into(),
        vector_lib::event::Value::Object(empty_map.clone()),
    );
    event2.insert(
        "tuple_with_map",
        vector_lib::event::Value::Object(empty_tuple_map),
    );

    let mut empty_tuple_complex = vector_lib::event::ObjectMap::new();
    empty_tuple_complex.insert("f0".into(), vector_lib::event::Value::Bytes("empty".into()));
    empty_tuple_complex.insert("f1".into(), vector_lib::event::Value::Array(vec![]));
    empty_tuple_complex.insert(
        "f2".into(),
        vector_lib::event::Value::Object(empty_map.clone()),
    );
    event2.insert(
        "tuple_with_nested",
        vector_lib::event::Value::Object(empty_tuple_complex),
    );

    event2.insert("locations", vector_lib::event::Value::Array(vec![]));
    event2.insert("tags_history", vector_lib::event::Value::Array(vec![]));
    event2.insert("metrics_history", vector_lib::event::Value::Array(vec![]));
    event2.insert(
        "request_headers",
        vector_lib::event::Value::Object(empty_map.clone()),
    );

    let mut empty_metrics = vector_lib::event::ObjectMap::new();
    empty_metrics.insert("f0".into(), vector_lib::event::Value::Integer(0));
    empty_metrics.insert("f1".into(), vector_lib::event::Value::Integer(0));
    empty_metrics.insert(
        "f2".into(),
        vector_lib::event::Value::Float(NotNan::new(0.0).unwrap()),
    );
    event2.insert(
        "response_metrics",
        vector_lib::event::Value::Object(empty_metrics),
    );

    event2.insert("tags", vector_lib::event::Value::Array(vec![]));
    event2.insert(
        "user_properties",
        vector_lib::event::Value::Object(empty_map),
    );
    event2.insert("array_with_nulls", vector_lib::event::Value::Array(vec![]));
    event2.insert(
        "array_with_named_tuple",
        vector_lib::event::Value::Array(vec![]),
    );

    events.push(event2.into());

    // Event 3: More varied data
    let mut event3 = LogEvent::from("Test varied data");
    event3.insert("host", "host3.example.com");

    event3.insert(
        "nested_int_array",
        vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Array(vec![]),
            vector_lib::event::Value::Array(vec![vector_lib::event::Value::Integer(99)]),
        ]),
    );
    event3.insert(
        "nested_string_array",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Array(vec![
            vector_lib::event::Value::Bytes("test".into()),
        ])]),
    );

    let mut map3 = vector_lib::event::ObjectMap::new();
    map3.insert(
        "colors".into(),
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Bytes("red".into())]),
    );
    event3.insert("array_map", vector_lib::event::Value::Object(map3));

    let mut int_map3 = vector_lib::event::ObjectMap::new();
    int_map3.insert(
        "values".into(),
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Integer(42)]),
    );
    event3.insert("int_array_map", vector_lib::event::Value::Object(int_map3));

    let mut tuple3 = vector_lib::event::ObjectMap::new();
    tuple3.insert("f0".into(), vector_lib::event::Value::Bytes("data".into()));
    tuple3.insert(
        "f1".into(),
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Integer(5)]),
    );
    event3.insert("tuple_with_array", vector_lib::event::Value::Object(tuple3));

    let mut map_inner = vector_lib::event::ObjectMap::new();
    map_inner.insert(
        "val".into(),
        vector_lib::event::Value::Float(NotNan::new(1.0).unwrap()),
    );
    let mut tuple_map3 = vector_lib::event::ObjectMap::new();
    tuple_map3.insert("f0".into(), vector_lib::event::Value::Bytes("test".into()));
    tuple_map3.insert("f1".into(), vector_lib::event::Value::Object(map_inner));
    event3.insert(
        "tuple_with_map",
        vector_lib::event::Value::Object(tuple_map3),
    );

    let mut map_inner2 = vector_lib::event::ObjectMap::new();
    map_inner2.insert(
        "x".into(),
        vector_lib::event::Value::Float(NotNan::new(2.0).unwrap()),
    );
    let mut tuple_nested3 = vector_lib::event::ObjectMap::new();
    tuple_nested3.insert("f0".into(), vector_lib::event::Value::Bytes("nest".into()));
    tuple_nested3.insert(
        "f1".into(),
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Integer(1)]),
    );
    tuple_nested3.insert("f2".into(), vector_lib::event::Value::Object(map_inner2));
    event3.insert(
        "tuple_with_nested",
        vector_lib::event::Value::Object(tuple_nested3),
    );

    let mut loc3 = vector_lib::event::ObjectMap::new();
    loc3.insert("f0".into(), vector_lib::event::Value::Bytes("NYC".into()));
    loc3.insert(
        "f1".into(),
        vector_lib::event::Value::Float(NotNan::new(40.7128).unwrap()),
    );
    loc3.insert(
        "f2".into(),
        vector_lib::event::Value::Float(NotNan::new(-74.0060).unwrap()),
    );
    event3.insert(
        "locations",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Object(loc3)]),
    );

    let mut tags3 = vector_lib::event::ObjectMap::new();
    tags3.insert("env".into(), vector_lib::event::Value::Bytes("dev".into()));
    event3.insert(
        "tags_history",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Object(tags3)]),
    );

    let mut metrics3 = vector_lib::event::ObjectMap::new();
    metrics3.insert("cpu".into(), vector_lib::event::Value::Integer(60));
    event3.insert(
        "metrics_history",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Object(metrics3)]),
    );

    let mut headers3 = vector_lib::event::ObjectMap::new();
    headers3.insert(
        "content-type".into(),
        vector_lib::event::Value::Bytes("application/json".into()),
    );
    event3.insert(
        "request_headers",
        vector_lib::event::Value::Object(headers3),
    );

    let mut metrics3_resp = vector_lib::event::ObjectMap::new();
    metrics3_resp.insert("f0".into(), vector_lib::event::Value::Integer(404));
    metrics3_resp.insert("f1".into(), vector_lib::event::Value::Integer(0));
    metrics3_resp.insert(
        "f2".into(),
        vector_lib::event::Value::Float(NotNan::new(0.001).unwrap()),
    );
    event3.insert(
        "response_metrics",
        vector_lib::event::Value::Object(metrics3_resp),
    );

    event3.insert(
        "tags",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Bytes("test".into())]),
    );

    let mut user_props3 = vector_lib::event::ObjectMap::new();
    user_props3.insert(
        "permissions".into(),
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Bytes("read".into())]),
    );
    event3.insert(
        "user_properties",
        vector_lib::event::Value::Object(user_props3),
    );

    event3.insert(
        "array_with_nulls",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Integer(42)]),
    );

    // Named tuple with single element
    let mut named_tuple3 = vector_lib::event::ObjectMap::new();
    named_tuple3.insert(
        "category".into(),
        vector_lib::event::Value::Bytes("status".into()),
    );
    named_tuple3.insert(
        "tag".into(),
        vector_lib::event::Value::Bytes("active".into()),
    );
    event3.insert(
        "array_with_named_tuple",
        vector_lib::event::Value::Array(vec![vector_lib::event::Value::Object(named_tuple3)]),
    );

    events.push(event3.into());

    run_and_assert_sink_compliance(sink, stream::iter(events), &SINK_TAGS).await;

    let output = client.select_all(&table).await;
    assert_eq!(3, output.rows);

    // Verify event 1 - comprehensive data
    let row1 = &output.data[0];
    assert!(
        row1.get("nested_int_array")
            .and_then(|v| v.as_array())
            .is_some()
    );
    assert!(row1.get("array_map").and_then(|v| v.as_object()).is_some());
    // Tuples are returned as arrays from ClickHouse
    assert!(
        row1.get("tuple_with_array")
            .and_then(|v| v.as_array())
            .is_some()
    );
    assert!(row1.get("locations").and_then(|v| v.as_array()).is_some());
    assert!(
        row1.get("tags_history")
            .and_then(|v| v.as_array())
            .is_some()
    );
    assert!(
        row1.get("request_headers")
            .and_then(|v| v.as_object())
            .is_some()
    );
    assert!(
        row1.get("array_with_nulls")
            .and_then(|v| v.as_array())
            .is_some()
    );

    // Verify event 2 - empty collections
    let row2 = &output.data[1];
    let empty_nested = row2
        .get("nested_int_array")
        .and_then(|v| v.as_array())
        .unwrap();
    assert_eq!(0, empty_nested.len());
    let empty_tags = row2.get("tags").and_then(|v| v.as_array()).unwrap();
    assert_eq!(0, empty_tags.len());

    // Verify event 3 - varied data
    let row3 = &output.data[2];
    let nested3 = row3
        .get("nested_int_array")
        .and_then(|v| v.as_array())
        .unwrap();
    assert_eq!(2, nested3.len());
}

/// Tests that missing required fields emit EncoderNullConstraintError and reject the batch
#[tokio::test]
async fn test_missing_required_field_emits_null_constraint_error() {
    init_test();

    let table = random_table_name();
    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let client = ClickhouseClient::new(host.clone());

    // Create table with non-nullable required_field column
    client
        .create_table(
            &table,
            "host String, timestamp DateTime64(3), message String, required_field String",
        )
        .await;

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: table.clone().try_into().unwrap(),
        compression: Compression::None,
        format: crate::sinks::clickhouse::config::Format::ArrowStream,
        batch_encoding: Some(BatchSerializerConfig::ArrowStream(Default::default())),
        batch,
        request: TowerRequestConfig {
            retry_attempts: 1,
            ..Default::default()
        },
        ..Default::default()
    };

    // Building the sink fetches the schema - required_field will be detected as non-nullable
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    // Create an event WITHOUT the required_field
    let (batch_notifier, mut receiver) = BatchNotifier::new_with_receiver();
    let mut event = LogEvent::from("test message").with_batch_notifier(&batch_notifier);
    drop(batch_notifier);
    event.insert("host", "example.com");
    // Deliberately NOT inserting "required_field"

    // Run the sink - should fail due to missing required field
    timeout(Duration::from_secs(5), sink.run_events(vec![event.into()]))
        .await
        .unwrap()
        .unwrap();

    // The batch should be rejected
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));

    // Verify the component_errors_total metric was incremented with the correct error_code
    let metrics = Controller::get().unwrap().capture_metrics();
    let null_constraint_errors: Vec<_> = metrics
        .iter()
        .filter(|m| {
            m.name() == "component_errors_total"
                && m.tags()
                    .map(|t| t.get("error_code") == Some("encoding_null_constraint"))
                    .unwrap_or(false)
        })
        .collect();

    assert!(
        !null_constraint_errors.is_empty(),
        "Expected component_errors_total with error_code=encoding_null_constraint to be emitted"
    );
}
