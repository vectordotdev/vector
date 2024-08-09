use std::collections::BTreeMap;

use futures::future::ready;
use futures::stream;

use databend_client::response::QueryResponse as DatabendAPIResponse;
use databend_client::APIClient as DatabendAPIClient;
use vector_lib::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent};

use crate::sinks::util::test::load_sink;
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::util::UriSerde,
    test_util::{
        components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS},
        random_string, trace_init,
    },
};

use super::config::DatabendConfig;

fn databend_endpoint() -> String {
    std::env::var("DATABEND_ENDPOINT")
        .unwrap_or_else(|_| "databend://vector:vector@databend:8000?sslmode=disable".into())
}

fn gen_table() -> String {
    format!("test_{}", random_string(10).to_lowercase())
}

fn make_event() -> (Event, BatchStatusReceiver) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let mut event = LogEvent::from("raw log line").with_batch_notifier(&batch);
    event.insert("host", "example.com");
    (event.into(), receiver)
}

async fn prepare_config(codec: &str, compression: &str) -> (String, String, DatabendAPIClient) {
    trace_init();

    let table = gen_table();
    let endpoint = databend_endpoint();
    let _endpoint: UriSerde = endpoint.parse().unwrap();

    let mut cfg = format!(
        r#"
            endpoint = "{}"
            table = "{}"
            batch.max_events = 1
        "#,
        endpoint, table,
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

async fn insert_event_with_cfg(cfg: String, table: String, client: DatabendAPIClient) {
    let create_table_sql = format!(
        "create table `{}` (host String, timestamp String, message String)",
        table
    );
    client.query(&create_table_sql).await.unwrap();

    let (config, _) = load_sink::<DatabendConfig>(&cfg).unwrap();
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (input_event, mut receiver) = make_event();
    run_and_assert_sink_compliance(
        sink,
        stream::once(ready(input_event.clone())),
        &HTTP_SINK_TAGS,
    )
    .await;

    let select_all_sql = format!("select * from `{}`", table);
    let resp = client.query(&select_all_sql).await.unwrap();
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
async fn insert_event_csv() {
    let (cfg, table, client) = prepare_config("csv", "none").await;
    insert_event_with_cfg(cfg, table, client).await;
}

#[tokio::test]
async fn insert_event_csv_gzip() {
    let (cfg, table, client) = prepare_config("csv", "gzip").await;
    insert_event_with_cfg(cfg, table, client).await;
}

fn response_to_map(resp: &DatabendAPIResponse) -> Vec<BTreeMap<String, Option<String>>> {
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
