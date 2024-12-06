use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{
        postgres::PostgresConfig,
        util::{test::load_sink, UriSerde},
    },
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

fn make_event() -> (Event, BatchStatusReceiver) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let mut event = LogEvent::from("raw log line").with_batch_notifier(&batch);
    event.insert("host", "example.com");
    let event_payload = event.clone().into_parts().0;
    event.insert("payload", event_payload);
    (event.into(), receiver)
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct TestEvent {
    host: String,
    timestamp: String,
    message: String,
    payload: serde_json::Value,
}

async fn prepare_config() -> (String, String, PgConnection) {
    trace_init();

    let table = temp_table();
    let endpoint: UriSerde = pg_url().parse().unwrap();

    let cfg = format!(
        r#"
            endpoint = "{endpoint}"
            table = "{table}"
            batch.max_events = 1
        "#,
    );

    let connection = PgConnection::connect(endpoint.to_string().as_str())
        .await
        .expect("Failed to connect to Postgres");

    (cfg, table, connection)
}
// TODO: create table that has an `insertion_date` that defaults to NOW in postgres, so we can order
// by it and get the event insertion order to check with the expected order.

async fn insert_event_with_cfg(cfg: String, table: String, mut connection: PgConnection) {
    // We store the timestamp as text and not as `timestamp with timezone` postgres type due to
    // postgres not supporting nanosecond-resolution (it does support microsecond-resolution).
    let create_table_sql =
        format!("CREATE TABLE IF NOT EXISTS {table} (host text, timestamp text, message text, payload jsonb)",);
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let (config, _) = load_sink::<PostgresConfig>(&cfg).unwrap();
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (input_event, mut receiver) = make_event();
    run_and_assert_sink_compliance(
        sink,
        stream::once(ready(input_event.clone())),
        &["endpoint", "protocol"],
    )
    .await;

    let select_all_sql = format!("SELECT * FROM {table}");
    let events: Vec<TestEvent> = sqlx::query_as(&select_all_sql)
        .fetch_all(&mut connection)
        .await
        .unwrap();
    assert_eq!(1, events.len());

    // drop input_event after comparing with response
    {
        let log_event = input_event.into_log();
        let expected = serde_json::to_value(&log_event).unwrap();
        let actual = serde_json::to_value(&events[0]).unwrap();
        assert_eq!(expected, actual);
    }

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
}

#[tokio::test]
async fn test_postgres_sink() {
    let (cfg, table, connection) = prepare_config().await;
    insert_event_with_cfg(cfg, table, connection).await;
}
