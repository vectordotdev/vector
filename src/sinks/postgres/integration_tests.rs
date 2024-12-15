use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{postgres::PostgresConfig, util::test::load_sink},
    test_util::{components::run_and_assert_sink_compliance, temp_table, trace_init},
};
use chrono::{DateTime, Utc};
use futures::stream;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, FromRow, PgConnection};
use std::future::ready;
use vector_lib::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent};

const POSTGRES_SINK_TAGS: [&str; 2] = ["endpoint", "protocol"];

fn pg_url() -> String {
    std::env::var("PG_URL").expect("PG_URL must be set")
}

fn create_event(id: i64) -> Event {
    let mut event = LogEvent::from("raw log line");
    event.insert("id", id);
    event.insert("host", "example.com");
    let event_payload = event.clone().into_parts().0;
    event.insert("payload", event_payload);
    let timestamp = Utc::now();
    // Postgres does not support nanosecond-resolution, so we truncate the timestamp to microsecond-resolution.
    // https://www.postgresql.org/docs/current/datatype-datetime.html
    let timestamp_microsecond_resolution =
        DateTime::from_timestamp_micros(timestamp.timestamp_micros());
    event.insert("timestamp", timestamp_microsecond_resolution);
    event.into()
}

fn create_event_with_notifier(id: i64) -> (Event, BatchStatusReceiver) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let event = create_event(id).with_batch_notifier(&batch);
    (event, receiver)
}

fn create_events(count: usize) -> (Vec<Event>, BatchStatusReceiver) {
    let mut events = (0..count as i64).map(create_event).collect::<Vec<_>>();
    let receiver = BatchNotifier::apply_to(&mut events);
    (events, receiver)
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct TestEvent {
    id: i64,
    host: String,
    timestamp: DateTime<Utc>,
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

#[tokio::test]
async fn insert_single_event() {
    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let create_table_sql =
        format!("CREATE TABLE {table} (id BIGINT, host TEXT, timestamp TIMESTAMPTZ, message TEXT, payload JSONB)");
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let (input_event, mut receiver) = create_event_with_notifier(0);
    let input_log_event = input_event.clone().into_log();
    let expected_value = serde_json::to_value(&input_log_event).unwrap();

    run_and_assert_sink_compliance(sink, stream::once(ready(input_event)), &POSTGRES_SINK_TAGS)
        .await;
    // We drop the event to notify the receiver that the batch was delivered.
    std::mem::drop(input_log_event);
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let select_all_sql = format!("SELECT * FROM {table}");
    let actual_event: TestEvent = sqlx::query_as(&select_all_sql)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    let actual_value = serde_json::to_value(actual_event).unwrap();
    assert_eq!(expected_value, actual_value);
}

#[tokio::test]
async fn insert_multiple_events() {
    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let create_table_sql = format!(
        "CREATE TABLE {table} (id BIGINT, host TEXT, timestamp TIMESTAMPTZ, message TEXT, payload JSONB)"
    );
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let (input_events, mut receiver) = create_events(100);
    let input_log_events = input_events
        .clone()
        .into_iter()
        .map(Event::into_log)
        .collect::<Vec<_>>();
    let expected_values = input_log_events
        .iter()
        .map(|event| serde_json::to_value(event).unwrap())
        .collect::<Vec<_>>();
    run_and_assert_sink_compliance(sink, stream::iter(input_events), &POSTGRES_SINK_TAGS).await;
    // We drop the event to notify the receiver that the batch was delivered.
    std::mem::drop(input_log_events);
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let select_all_sql = format!("SELECT * FROM {table} ORDER BY id");
    let actual_events: Vec<TestEvent> = sqlx::query_as(&select_all_sql)
        .fetch_all(&mut connection)
        .await
        .unwrap();
    let actual_values = actual_events
        .iter()
        .map(|event| serde_json::to_value(event).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(expected_values, actual_values);
}

// Using null::{table} with jsonb_populate_recordset does not work with default values.
// it is like inserting null values explicitly, it does not use table's default values.
// https://dba.stackexchange.com/questions/308114/use-default-value-instead-of-inserted-null
// https://stackoverflow.com/questions/49992531/postgresql-insert-a-null-convert-to-default
// TODO: this cannot be fixed without a workaround involving a trigger creation, which is beyond
// Vector's job in the DB. We should document this limitation alongside with this test.
#[tokio::test]
async fn default_columns_are_not_populated() {
    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let create_table_sql = format!(
        "CREATE TABLE {table} (id BIGINT, not_existing_column TEXT DEFAULT 'default_value')"
    );
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let (input_event, mut receiver) = create_event_with_notifier(0);
    run_and_assert_sink_compliance(
        sink,
        stream::once(ready(input_event.clone())),
        &POSTGRES_SINK_TAGS,
    )
    .await;
    // We drop the event to notify the receiver that the batch was delivered.
    std::mem::drop(input_event);
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let select_all_sql = format!("SELECT not_existing_column FROM {table}");
    let inserted_not_existing_column: (Option<String>,) = sqlx::query_as(&select_all_sql)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(inserted_not_existing_column.0, None);
}
