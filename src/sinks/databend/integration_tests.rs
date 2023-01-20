use futures::future::ready;
use futures::stream;

use vector_common::sensitive_string::SensitiveString;
use vector_core::config::proxy::ProxyConfig;
use vector_core::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent};

use crate::config::{SinkConfig, SinkContext};
use crate::http::{Auth, HttpClient};
use crate::sinks::databend::api::{DatabendAPIClient, DatabendHttpRequest};
use crate::sinks::databend::DatabendConfig;
use crate::sinks::util::{BatchConfig, Compression, TowerRequestConfig, UriSerde};
use crate::test_util::components::{run_and_assert_sink_compliance, HTTP_SINK_TAGS};
use crate::test_util::{random_string, trace_init};

fn databend_endpoint() -> String {
    std::env::var("DATABEND_ENDPOINT").unwrap_or_else(|_| "http://localhost:8000".into())
}

fn databend_user() -> String {
    std::env::var("DATABEND_USER").unwrap_or_else(|_| "vector".into())
}

fn databend_password() -> String {
    std::env::var("DATABEND_PASSWORD").unwrap_or_else(|_| "vector".into())
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

#[tokio::test]
async fn insert_events() {
    trace_init();

    let table = gen_table();
    let endpoint: UriSerde = databend_endpoint().parse().unwrap();
    let auth = Some(Auth::Basic {
        user: databend_user(),
        password: SensitiveString::from(databend_password()),
    });

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = DatabendConfig {
        endpoint: endpoint.clone(),
        table: table.clone(),
        compression: Compression::None,
        batch,
        auth: auth.clone(),
        request: TowerRequestConfig {
            retry_attempts: Some(1),
            ..Default::default()
        },
        ..Default::default()
    };

    let proxy = ProxyConfig::default();
    let http_client = HttpClient::new(None, &proxy).unwrap();

    let client = DatabendAPIClient::new(http_client, endpoint, auth);

    let create_table_sql = format!(
        "create table `{}` (host String, timestamp String, message String, items Array(String))",
        table
    );
    client
        .query(DatabendHttpRequest::new(create_table_sql))
        .await
        .unwrap();

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

    let select_all_sql = format!("select * from `{}`", table);

    let resp = client
        .query(DatabendHttpRequest::new(select_all_sql))
        .await
        .unwrap();
    assert_eq!(1, resp.data.len());

    let log_event = input_event.into_log();
    assert_eq!(log_event["host"].to_string(), resp.data[0][0].to_string());
    assert_eq!(
        log_event["timestamp"].to_string(),
        resp.data[0][1].to_string()
    );
    assert_eq!(
        log_event["message"].to_string(),
        resp.data[0][2].to_string()
    );
    assert_eq!(log_event["items"].to_string(), resp.data[0][3].to_string());

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
}
