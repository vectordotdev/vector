use crate::test_util::integration::postgres::pg_url;
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{postgres::PostgresConfig, util::test::load_sink},
    test_util::{
        components::{
            run_and_assert_sink_compliance, run_and_assert_sink_error, COMPONENT_ERROR_TAGS,
        },
        temp_table, trace_init,
    },
};
use chrono::{DateTime, Utc};
use futures::stream;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, FromRow, PgConnection};
use std::future::ready;
use vector_lib::event::{
    BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent, Metric, MetricKind,
    MetricValue,
};

const POSTGRES_SINK_TAGS: [&str; 2] = ["endpoint", "protocol"];

fn timestamp() -> DateTime<Utc> {
    let timestamp = Utc::now();
    // Postgres does not support nanosecond-resolution, so we truncate the timestamp to microsecond-resolution.
    // https://www.postgresql.org/docs/current/datatype-datetime.html
    DateTime::from_timestamp_micros(timestamp.timestamp_micros()).unwrap()
}

fn create_event(id: i64) -> Event {
    let mut event = LogEvent::from("raw log line");
    event.insert("id", id);
    event.insert("host", "example.com");
    let event_payload = event.clone().into_parts().0;
    event.insert("payload", event_payload);
    event.insert("timestamp", timestamp());
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

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct TestCounterMetric {
    name: String,
    namespace: String,
    tags: serde_json::Value,
    timestamp: DateTime<Utc>,
    kind: String,
    counter: serde_json::Value,
}

async fn prepare_config() -> (PostgresConfig, String, PgConnection) {
    let table = temp_table();
    let endpoint = pg_url();
    let config_str = format!(
        r#"
            endpoint = "{endpoint}"
            table = "{table}"
            batch.max_events = 1
        "#,
    );
    let (config, _) = load_sink::<PostgresConfig>(&config_str).unwrap();

    let connection = PgConnection::connect(endpoint.as_str())
        .await
        .expect("Failed to connect to Postgres");

    (config, table, connection)
}

#[tokio::test]
async fn healthcheck_passes() {
    trace_init();
    let (config, _table, _connection) = prepare_config().await;
    let (_sink, healthcheck) = config.build(SinkContext::default()).await.unwrap();
    healthcheck.await.unwrap();
}

// This test does not actually fail in the healthcheck query, but in the connection pool creation at
// `PostgresConfig::build`
#[tokio::test]
async fn healthcheck_fails() {
    trace_init();

    let table = temp_table();
    let endpoint = "postgres://user:pass?host=/unknown_socket_path".to_string();
    let config_str = format!(
        r#"
            endpoint = "{endpoint}"
            table = "{table}"
        "#,
    );
    let (config, _) = load_sink::<PostgresConfig>(&config_str).unwrap();

    assert!(config.build(SinkContext::default()).await.is_err());
}

#[tokio::test]
async fn insert_single_event() {
    trace_init();

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
    trace_init();

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

#[tokio::test]
async fn insert_metric() {
    trace_init();

    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let create_table_sql = format!(
        "CREATE TABLE {table} (name TEXT, namespace TEXT, tags JSONB, timestamp TIMESTAMP WITH TIME ZONE,
        kind TEXT, counter JSONB)"
    );
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let metric = Metric::new(
        "counter",
        MetricKind::Absolute,
        MetricValue::Counter { value: 1.0 },
    )
    .with_namespace(Some("vector"))
    .with_tags(Some(metric_tags!("some_tag" => "some_value")))
    .with_timestamp(Some(timestamp()));
    let expected_metric_value = serde_json::to_value(&metric).unwrap();
    let input_event = Event::from(metric);

    run_and_assert_sink_compliance(sink, stream::once(ready(input_event)), &POSTGRES_SINK_TAGS)
        .await;

    let select_all_sql = format!("SELECT * FROM {table}");
    let inserted_metric: TestCounterMetric = sqlx::query_as(&select_all_sql)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    let inserted_metric_value = serde_json::to_value(&inserted_metric).unwrap();
    assert_eq!(inserted_metric_value, expected_metric_value);
}

// Using null::{table} with jsonb_populate_recordset does not work with default values.
// it is like inserting null values explicitly, it does not use table's default values.
// https://dba.stackexchange.com/questions/308114/use-default-value-instead-of-inserted-null
// https://stackoverflow.com/questions/49992531/postgresql-insert-a-null-convert-to-default
// TODO: this cannot be fixed without a workaround involving a trigger creation, which is beyond
// Vector's job in the DB. We should document this limitation alongside with this test.
#[tokio::test]
async fn default_columns_are_not_populated() {
    trace_init();

    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let create_table_sql = format!(
        "CREATE TABLE {table} (id BIGINT, not_existing_field TEXT DEFAULT 'default_value')"
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

    let select_all_sql = format!("SELECT not_existing_field FROM {table}");
    let inserted_not_existing_field: (Option<String>,) = sqlx::query_as(&select_all_sql)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(inserted_not_existing_field.0, None);
}

#[tokio::test]
async fn extra_fields_are_ignored() {
    trace_init();

    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let create_table_sql = format!("CREATE TABLE {table} (message TEXT)");
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let (input_event, mut receiver) = create_event_with_notifier(0);
    let input_log_event = input_event.clone().into_log();
    let expected_value = input_log_event
        .get_message()
        .unwrap()
        .as_str()
        .unwrap()
        .into_owned();

    run_and_assert_sink_compliance(sink, stream::once(ready(input_event)), &POSTGRES_SINK_TAGS)
        .await;
    // We drop the event to notify the receiver that the batch was delivered.
    std::mem::drop(input_log_event);
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let select_all_sql = format!("SELECT * FROM {table}");
    let actual_value: (String,) = sqlx::query_as(&select_all_sql)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(expected_value, actual_value.0);
}

#[tokio::test]
async fn insertion_fails_required_field_is_not_present() {
    trace_init();

    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let create_table_sql =
        format!("CREATE TABLE {table} (message TEXT, not_existing_field TEXT NOT NULL)");
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let (input_event, mut receiver) = create_event_with_notifier(0);

    run_and_assert_sink_error(
        sink,
        stream::once(ready(input_event.clone())),
        &COMPONENT_ERROR_TAGS,
    )
    .await;
    // We drop the event to notify the receiver that the batch was delivered.
    std::mem::drop(input_event);
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));

    // We ensure that the event was not inserted.
    let select_all_sql = format!("SELECT * FROM {table}");
    let first_row: Option<(String, String)> = sqlx::query_as(&select_all_sql)
        .fetch_optional(&mut connection)
        .await
        .unwrap();
    assert_eq!(first_row, None);
}

#[tokio::test]
async fn insertion_fails_missing_table() {
    trace_init();

    let table = "missing_table".to_string();
    let (mut config, _, _) = prepare_config().await;
    config.table = table.clone();

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let (input_event, mut receiver) = create_event_with_notifier(0);

    run_and_assert_sink_error(
        sink,
        stream::once(ready(input_event)),
        &COMPONENT_ERROR_TAGS,
    )
    .await;
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));
}
