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
