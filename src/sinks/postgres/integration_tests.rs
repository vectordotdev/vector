use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{postgres::PostgresConfig, util::test::load_sink},
    test_util::{components::run_and_assert_sink_compliance, temp_table, trace_init},
};
use futures::stream;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, FromRow, PgConnection};
use std::future::ready;
use vector_lib::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent};

fn pg_url() -> String {
    std::env::var("PG_URL").expect("PG_URL must be set")
}

fn create_event(id: i64) -> Event {
    let mut event = LogEvent::from("raw log line");
    event.insert("id", id);
    event.insert("host", "example.com");
    let event_payload = event.clone().into_parts().0;
    event.insert("payload", event_payload);
    event.into()
}

fn create_event_with_notifier(id: i64) -> (Event, BatchStatusReceiver) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let event = create_event(id).with_batch_notifier(&batch);
    (event, receiver)
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct TestEvent {
    id: i64,
    host: String,
    timestamp: String,
    message: String,
    payload: serde_json::Value,
}

async fn prepare_config() -> (PostgresConfig, String, PgConnection) {
    trace_init();

    let table = temp_table();
    let endpoint = pg_url();

    let config_str = format!(
        r#"
            endpoint = "{endpoint}"
            table = "{table}"
        "#,
    );
    let (config, _) = load_sink::<PostgresConfig>(&config_str).unwrap();

    let connection = PgConnection::connect(endpoint.as_str())
        .await
        .expect("Failed to connect to Postgres");

    (config, table, connection)
}

// TODO: create table that has an `insertion_date` that defaults to NOW in postgres, so we can order
// by it and get the event insertion order to check with the expected order.
#[tokio::test]
async fn insert_single_event() {
    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    // We store the timestamp as text and not as `timestamp with timezone` postgres type due to
    // postgres not supporting nanosecond-resolution (it does support microsecond-resolution).
    let create_table_sql =
        format!("CREATE TABLE IF NOT EXISTS {table} (id bigint, host text, timestamp text, message text, payload jsonb)");
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let (input_event, mut receiver) = create_event_with_notifier(0);
    run_and_assert_sink_compliance(
        sink,
        stream::once(ready(input_event.clone())),
        &["endpoint", "protocol"],
    )
    .await;

    let select_all_sql = format!("SELECT * FROM {table}");
    let actual_event: TestEvent = sqlx::query_as(&select_all_sql)
        .fetch_one(&mut connection)
        .await
        .unwrap();

    // drop input_event after comparing with response
    {
        let input_log_event = input_event.into_log();
        let expected_value = serde_json::to_value(&input_log_event).unwrap();
        let actual_value = serde_json::to_value(actual_event).unwrap();
        assert_eq!(expected_value, actual_value);
    }

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
}
