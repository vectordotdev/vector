use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use futures::{
    future::{ok, ready},
    stream,
};
use http::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use tokio::time::{timeout, Duration};
use vector_core::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent};
use warp::Filter;

use super::*;
use crate::sinks::clickhouse::ClickhouseConfig;
use crate::{
    codecs::{TimestampFormat, Transformer},
    config::{log_schema, SinkConfig, SinkContext},
    sinks::util::{BatchConfig, Compression, TowerRequestConfig},
    test_util::{
        components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
        random_string, trace_init,
    },
};

fn clickhouse_address() -> String {
    std::env::var("CLICKHOUSE_ADDRESS").unwrap_or_else(|_| "tcp://localhost:9000".into())
}

#[tokio::test]
async fn insert_events() {
    trace_init();

    let table = gen_table();
    let host = clickhouse_address();

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = ClickhouseConfig {
        endpoint: host.parse().unwrap(),
        table: table.clone(),
        compression: Compression::None,
        batch,
        request: TowerRequestConfig {
            retry_attempts: Some(1),
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

    let (sink, _hc) = config.build(SinkContext::new_test()).await.unwrap();

    let (mut input_event, mut receiver) = make_event();
    input_event
        .as_mut_log()
        .insert("items", vec!["item1", "item2"]);

    run_and_assert_sink_compliance(
        sink,
        stream::once(ready(input_event.clone())),
        &HTTP_SINK_TAGS,
    )
    .await;

    let output = client.select_all(&table).await;
    assert_eq!(1, output.rows);

    let expected = serde_json::to_value(input_event.into_log()).unwrap();
    assert_eq!(expected, output.data[0]);

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
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
            //
            .body(format!(
                "CREATE TABLE {}
                    ({})
                    ENGINE = MergeTree()
                    ORDER BY (host, timestamp);",
                table, schema
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
            .body(format!("SELECT * FROM {} FORMAT JSON", table))
            .send()
            .await
            .unwrap();

        if !response.status().is_success() {
            panic!("select all failed: {}", response.text().await.unwrap())
        } else {
            let text = response.text().await.unwrap();
            match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(_) => panic!("json failed: {:?}", text),
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

fn gen_table() -> String {
    format!("test_{}", random_string(10).to_lowercase())
}
