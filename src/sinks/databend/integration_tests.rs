use std::{collections::BTreeMap, sync::Arc};

use databend_client::{APIClient as DatabendAPIClient, Page};
use futures::{future::ready, stream};
use vector_lib::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent, Value};

use super::config::DatabendConfig;
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::util::{UriSerde, test::load_sink},
    test_util::{
        components::{HTTP_SINK_TAGS, run_and_assert_sink_compliance},
        random_table_name, trace_init,
    },
};

fn databend_endpoint() -> String {
    std::env::var("DATABEND_ENDPOINT")
        .unwrap_or_else(|_| "databend://vector:vector@databend:8000?sslmode=disable".into())
}

fn make_event() -> (Event, BatchStatusReceiver) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let mut event = LogEvent::from("raw log line").with_batch_notifier(&batch);
    event.insert("host", "example.com");
    (event.into(), receiver)
}

async fn prepare_config(
    codec: &str,
    compression: &str,
) -> (String, String, Arc<DatabendAPIClient>) {
    trace_init();

    let table = random_table_name();
    let endpoint = databend_endpoint();
    let _endpoint: UriSerde = endpoint.parse().unwrap();

    let mut cfg = format!(
        r#"
            endpoint = "{endpoint}"
            table = "{table}"
            batch.max_events = 1
        "#,
    );
    match codec {
        "json" => {
            cfg.push_str(
                r#"
                    encoding.codec = "json"
                "#,
            );
        }
        "csv" => {
            cfg.push_str(
                r#"
                    encoding.codec = "csv"
                    encoding.csv.fields = ["host", "timestamp", "message"]
                "#,
            );
        }
        _ => panic!("unsupported codec"),
    }
    match compression {
        "gzip" => {
            cfg.push_str(
                r#"
                    compression = "gzip"
                "#,
            );
        }
        "zstd" => {
            cfg.push_str(
                r#"
                    compression = "zstd"
                "#,
            );
        }
        "none" => {
            cfg.push_str(
                r#"
                    compression = "none"
                "#,
            );
        }
        _ => panic!("unsupported codec"),
    }

    let client = DatabendAPIClient::new(&endpoint, Some("vector/integration-test".to_string()))
        .await
        .unwrap();

    (cfg, table, client)
}

fn make_replace_event(id: i64, source: &str, value: &str) -> Event {
    let mut event = LogEvent::default();
    event.insert("id", id);
    event.insert("source", source);
    event.insert("value", value);
    event.into()
}

fn make_copy_error_event(id: Value, value: &str) -> Event {
    let mut event = LogEvent::default();
    event.insert("id", id);
    event.insert("value", value);
    event.into()
}

async fn insert_event_with_cfg(cfg: String, table: String, client: Arc<DatabendAPIClient>) {
    let create_table_sql =
        format!("create table `{table}` (host String, timestamp String, message String)");
    client.query_all(&create_table_sql, None).await.unwrap();

    let (config, _) = load_sink::<DatabendConfig>(&cfg).unwrap();
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (input_event, mut receiver) = make_event();
    run_and_assert_sink_compliance(
        sink,
        stream::once(ready(input_event.clone())),
        &HTTP_SINK_TAGS,
    )
    .await;

    let select_all_sql = format!("select * from `{table}`");
    let resp = client.query_all(&select_all_sql, None).await.unwrap();
    assert_eq!(1, resp.data.len());

    // drop input_event after comparing with response
    {
        let log_event = input_event.into_log();
        let expected = serde_json::to_string(&log_event).unwrap();
        let resp_data = response_to_map(&resp);
        let actual = serde_json::to_string(&resp_data[0]).unwrap();
        assert_eq!(expected, actual);
    }

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
}

#[tokio::test]
async fn insert_event_json() {
    let (cfg, table, client) = prepare_config("json", "none").await;
    insert_event_with_cfg(cfg, table, client).await;
}

#[tokio::test]
async fn insert_event_json_gzip() {
    let (cfg, table, client) = prepare_config("json", "gzip").await;
    insert_event_with_cfg(cfg, table, client).await;
}

#[tokio::test]
async fn insert_event_json_zstd() {
    let (cfg, table, client) = prepare_config("json", "zstd").await;
    insert_event_with_cfg(cfg, table, client).await;
}

#[tokio::test]
async fn insert_event_csv() {
    let (cfg, table, client) = prepare_config("csv", "none").await;
    insert_event_with_cfg(cfg, table, client).await;
}

#[tokio::test]
async fn insert_event_csv_gzip() {
    let (cfg, table, client) = prepare_config("csv", "gzip").await;
    insert_event_with_cfg(cfg, table, client).await;
}

#[tokio::test]
async fn staged_copy_on_error_continue_skips_bad_rows() {
    trace_init();

    let table = random_table_name();
    let endpoint = databend_endpoint();
    let client = DatabendAPIClient::new(&endpoint, Some("vector/integration-test".to_string()))
        .await
        .unwrap();
    client
        .query_all(
            &format!("create table `{table}` (id Int64, value String)"),
            None,
        )
        .await
        .unwrap();

    let cfg = format!(
        r#"
            endpoint = "{endpoint}"
            table = "{table}"
            batch.max_events = 2
            encoding.codec = "csv"
            encoding.csv.fields = ["id", "value"]
            copy_options.on_error = "continue"
        "#,
    );
    let (config, _) = load_sink::<DatabendConfig>(&cfg).unwrap();
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    run_and_assert_sink_compliance(
        sink,
        stream::iter(vec![
            make_copy_error_event(1.into(), "good"),
            make_copy_error_event("not-an-int".into(), "bad"),
        ]),
        &HTTP_SINK_TAGS,
    )
    .await;

    let resp = client
        .query_all(
            &format!("select id, value from `{table}` order by id"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(1, resp.data.len());
    assert_eq!(Some("1".to_string()), resp.data[0][0]);
    assert_eq!(Some("good".to_string()), resp.data[0][1]);
}

#[tokio::test]
async fn replace_event_with_primary_key() {
    trace_init();

    let table = random_table_name();
    let endpoint = databend_endpoint();
    let client = DatabendAPIClient::new(&endpoint, Some("vector/integration-test".to_string()))
        .await
        .unwrap();
    client
        .query_all(
            &format!("create table `{table}` (id Int64, source String, value String)"),
            None,
        )
        .await
        .unwrap();

    let cfg = format!(
        r#"
            endpoint = "{endpoint}"
            table = "{table}"
            batch.max_events = 1
            encoding.codec = "json"
            compression = "zstd"
            primary_key = ["id", "source"]
        "#,
    );
    let (config, _) = load_sink::<DatabendConfig>(&cfg).unwrap();
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    run_and_assert_sink_compliance(
        sink,
        stream::iter(vec![
            make_replace_event(1, "file", "old"),
            make_replace_event(1, "file", "new"),
            make_replace_event(2, "file", "steady"),
        ]),
        &HTTP_SINK_TAGS,
    )
    .await;

    let resp = client
        .query_all(
            &format!("select id, source, value from `{table}` order by id"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(2, resp.data.len());
    assert_eq!(Some("1".to_string()), resp.data[0][0]);
    assert_eq!(Some("file".to_string()), resp.data[0][1]);
    assert_eq!(Some("new".to_string()), resp.data[0][2]);
    assert_eq!(Some("2".to_string()), resp.data[1][0]);
    assert_eq!(Some("steady".to_string()), resp.data[1][2]);
}

fn response_to_map(resp: &Page) -> Vec<BTreeMap<String, Option<String>>> {
    let mut result = Vec::new();
    for row in &resp.data {
        let mut map = BTreeMap::new();
        for (i, field) in resp.schema.iter().enumerate() {
            map.insert(field.name.clone(), row[i].clone());
        }
        result.push(map);
    }
    result
}
