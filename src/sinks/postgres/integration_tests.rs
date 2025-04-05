use crate::test_util::integration::postgres::pg_url;
use crate::{
    config::{SinkConfig, SinkContext},
    event::{ObjectMap, TraceEvent, Value},
    sinks::{postgres::PostgresConfig, util::test::load_sink},
    test_util::{
        components::{
            run_and_assert_sink_compliance, run_and_assert_sink_error, COMPONENT_ERROR_TAGS,
        },
        random_table_name, trace_init,
    },
};
use chrono::{DateTime, Utc};
use futures::stream;
use ordered_float::NotNan;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, FromRow, PgConnection};
use std::future::ready;
use vector_lib::event::{
    BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent, Metric, MetricKind,
    MetricValue,
};
use vrl::event_path;

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

fn create_metric(name: &str) -> Metric {
    Metric::new(
        name,
        MetricKind::Absolute,
        MetricValue::Counter { value: 1.0 },
    )
    .with_namespace(Some("vector"))
    .with_tags(Some(metric_tags!("some_tag" => "some_value")))
    .with_timestamp(Some(timestamp()))
}

fn create_span(resource: &str) -> ObjectMap {
    ObjectMap::from([
        ("service".into(), Value::from("a_service")),
        ("name".into(), Value::from("a_name")),
        ("resource".into(), Value::from(resource)),
        ("type".into(), Value::from("a_type")),
        ("trace_id".into(), Value::Integer(123)),
        ("span_id".into(), Value::Integer(456)),
        ("parent_id".into(), Value::Integer(789)),
        ("start".into(), Value::from(timestamp())),
        ("duration".into(), Value::Integer(1_000_000)),
        ("error".into(), Value::Integer(404)),
        (
            "meta".into(),
            Value::Object(ObjectMap::from([
                ("foo".into(), Value::from("bar")),
                ("bar".into(), Value::from("baz")),
            ])),
        ),
        (
            "metrics".into(),
            Value::Object(ObjectMap::from([
                ("a_metric".into(), Value::Float(NotNan::new(0.577).unwrap())),
                ("_top_level".into(), Value::Float(NotNan::new(1.0).unwrap())),
            ])),
        ),
    ])
}

pub fn create_trace(resource: &str) -> TraceEvent {
    let mut t = TraceEvent::default();
    t.insert(event_path!("trace_id"), Value::Integer(123));
    t.insert(
        event_path!("spans"),
        Value::Array(vec![Value::from(create_span(resource))]),
    );
    t
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

#[derive(Debug, Serialize, Deserialize, FromRow)]
struct TestTrace {
    trace_id: i64,
    spans: Vec<TestTraceSpan>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, FromRow)]
#[sqlx(type_name = "trace_span")]
struct TestTraceSpan {
    service: String,
    name: String,
    resource: String,
    r#type: String,
    trace_id: i64,
    span_id: i64,
    parent_id: i64,
    start: DateTime<Utc>,
    duration: i64,
    error: i64,
    meta: serde_json::Value,
    metrics: serde_json::Value,
}

async fn prepare_config() -> (PostgresConfig, String, PgConnection) {
    let table = random_table_name();
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

    let table = random_table_name();
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
        "CREATE TABLE {table} (name TEXT, namespace TEXT, tags JSONB, timestamp TIMESTAMPTZ,
        kind TEXT, counter JSONB)"
    );
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let metric = create_metric("counter");
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

#[tokio::test]
async fn insert_trace() {
    trace_init();

    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let drop_type_sql = "DROP TYPE IF EXISTS trace_span CASCADE";
    sqlx::query(drop_type_sql)
        .execute(&mut connection)
        .await
        .unwrap();
    let create_trace_span_type_sql = "CREATE TYPE trace_span AS
        (service TEXT, name TEXT, resource TEXT, type TEXT, trace_id BIGINT,
        span_id BIGINT, parent_id BIGINT, start TIMESTAMPTZ, duration BIGINT,
        error BIGINT, meta JSONB, metrics JSONB)";
    sqlx::query(create_trace_span_type_sql)
        .execute(&mut connection)
        .await
        .unwrap();
    let create_table_sql = format!("CREATE TABLE {table} (trace_id BIGINT, spans trace_span[])");
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let trace = create_trace("a_resource");
    let expected_trace_value = serde_json::to_value(&trace).unwrap();
    let input_event = Event::from(trace);

    run_and_assert_sink_compliance(sink, stream::once(ready(input_event)), &POSTGRES_SINK_TAGS)
        .await;

    let select_all_sql = format!("SELECT * FROM {table}");
    let inserted_trace: TestTrace = sqlx::query_as(&select_all_sql)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    let inserted_trace_value = serde_json::to_value(&inserted_trace).unwrap();
    assert_eq!(inserted_trace_value, expected_trace_value);
}

// Using null::{table} with jsonb_populate_recordset does not work well with default values,
// it is like inserting null values explicitly, it doesn't use table's default values.
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

#[tokio::test]
async fn insertion_fails_primary_key_violation() {
    trace_init();

    let (config, table, mut connection) = prepare_config().await;
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    let create_table_sql =
        format!("CREATE TABLE {table} (id BIGINT PRIMARY KEY, host TEXT, timestamp TIMESTAMPTZ, message TEXT, payload JSONB)");
    sqlx::query(&create_table_sql)
        .execute(&mut connection)
        .await
        .unwrap();

    let event = create_event(0);
    run_and_assert_sink_error(
        sink,
        // We send the same event twice to trigger a primary key violation on column `id`.
        stream::iter(vec![event.clone(), event]),
        &COMPONENT_ERROR_TAGS,
    )
    .await;
}
