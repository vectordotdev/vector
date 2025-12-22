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
use serde::Deserialize;
use serde_json::Value;
use tokio::time::{Duration, timeout};
use vector_lib::{
    codecs::encoding::BatchSerializerConfig,
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
        components::{SINK_TAGS, run_and_assert_sink_compliance},
        random_table_name, trace_init,
    },
};

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
