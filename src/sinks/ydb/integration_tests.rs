use chrono::{DateTime, Utc};
use futures::stream;
use tower::Service;
use vector_lib::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event, LogEvent};
use ydb::{ClientBuilder, Query, TableClient};

use crate::{
    config::{SinkConfig, SinkContext},
    sinks::{util::test::load_sink, ydb::YdbConfig},
    test_util::{components::run_and_assert_sink_compliance, random_table_name, trace_init},
};

const YDB_SINK_TAGS: [&str; 2] = ["endpoint", "protocol"];

fn ydb_endpoint() -> String {
    std::env::var("YDB_ENDPOINT").unwrap_or_else(|_| "grpc://localhost:2136?database=/local".into())
}

fn timestamp() -> DateTime<Utc> {
    Utc::now()
}

fn create_event(id: i64) -> Event {
    let mut event = LogEvent::from("test log message");
    event.insert("id", id);
    event.insert("host", "test-host.example.com");
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

struct YdbTestClient {
    client: ydb::Client,
    table_client: TableClient,
}

impl YdbTestClient {
    async fn new(endpoint: &str) -> Self {
        let client = ClientBuilder::new_from_connection_string(endpoint)
            .expect("Failed to parse YDB connection string")
            .client()
            .expect("Failed to create YDB client");

        client.wait().await.expect("Failed to connect to YDB");

        let table_client = client.table_client();

        Self {
            client,
            table_client,
        }
    }

    fn database(&self) -> String {
        self.client.database().to_string()
    }

    async fn create_table(&self, table_path: &str) {
        let create_table_sql = format!(
            r#"CREATE TABLE `{}` (
                id Int64 NOT NULL,
                host Utf8,
                timestamp Timestamp,
                message Utf8,
                PRIMARY KEY(id)
            )"#,
            table_path
        );

        self.table_client
            .retry_execute_scheme_query(create_table_sql)
            .await
            .expect("Failed to create YDB table");
    }

    async fn drop_table(&self, table_path: &str) {
        let drop_table_sql = format!("DROP TABLE `{}`", table_path);
        let _ = self
            .table_client
            .retry_execute_scheme_query(drop_table_sql)
            .await;
    }

    async fn count_rows(&self, table_path: &str) -> u64 {
        let table_client = self.table_client.clone_with_transaction_options(
            ydb::TransactionOptions::new()
                .with_mode(ydb::Mode::OnlineReadonly)
                .with_autocommit(true),
        );

        let table_path = table_path.to_string();
        table_client
            .retry_transaction(|mut t| {
                let table = table_path.clone();
                async move {
                    let select_query = format!("SELECT COUNT(*) as cnt FROM `{}`", table);
                    let result_set = t.query(Query::new(&select_query)).await?;
                    let value = result_set.into_only_row()?.remove_field_by_name("cnt")?;
                    let cnt: Option<u64> = value.try_into()?;
                    Ok(cnt.unwrap_or(0))
                }
            })
            .await
            .expect("Failed to count rows")
    }
}

async fn prepare_config() -> (YdbConfig, String, YdbTestClient) {
    let endpoint = ydb_endpoint();
    let client = YdbTestClient::new(&endpoint).await;

    let database = client.database();
    let table_name = random_table_name();
    let table = format!("{}/{}", database, table_name);

    let config_str = format!(
        r#"
            endpoint = "{endpoint}"
            table = "{table}"
            batch.max_events = 1
        "#,
    );
    let (config, _) = load_sink::<YdbConfig>(&config_str).expect("Failed to parse config");

    (config, table, client)
}

#[tokio::test]
async fn healthcheck_passes() {
    trace_init();

    let (config, table, client) = prepare_config().await;
    client.create_table(&table).await;

    let (_sink, healthcheck) = config
        .build(SinkContext::default())
        .await
        .expect("sink should build successfully");

    assert!(healthcheck.await.is_ok());

    client.drop_table(&table).await;
}

#[tokio::test]
async fn insert_single_event() {
    trace_init();

    let (config, table, client) = prepare_config().await;
    client.create_table(&table).await;

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (input_event, mut receiver) = create_event_with_notifier(0);

    run_and_assert_sink_compliance(sink, stream::iter(vec![input_event]), &YDB_SINK_TAGS).await;

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let count = client.count_rows(&table).await;
    assert_eq!(count, 1, "Expected 1 row");

    client.drop_table(&table).await;
}

#[tokio::test]
async fn insert_multiple_events() {
    trace_init();

    let (config, table, client) = prepare_config().await;
    client.create_table(&table).await;

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (input_events, mut receiver) = create_events(150);

    run_and_assert_sink_compliance(sink, stream::iter(input_events), &YDB_SINK_TAGS).await;

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let count = client.count_rows(&table).await;
    assert_eq!(count, 150, "Expected 150 events");

    client.drop_table(&table).await;
}

#[tokio::test]
async fn dynamic_mapping_with_various_types() {
    trace_init();

    let (config, table, client) = prepare_config().await;

    let create_table_sql = format!(
        r#"CREATE TABLE `{}` (
            id Int64 NOT NULL,
            name Utf8,
            score Double,
            active Bool,
            created_at Timestamp,
            metadata JsonDocument,
            PRIMARY KEY(id)
        )"#,
        table
    );

    client
        .table_client
        .retry_execute_scheme_query(create_table_sql)
        .await
        .expect("Failed to create table");

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let mut event = LogEvent::from("dynamic mapping test");
    event.insert("id", 42_i64);
    event.insert("name", "test-user");
    event.insert("score", 99.5_f64);
    event.insert("active", true);
    event.insert("created_at", timestamp());
    event.insert("metadata", serde_json::json!({"key": "value", "count": 10}));

    let (input_event, mut receiver) = {
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        (Event::from(event).with_batch_notifier(&batch), receiver)
    };

    run_and_assert_sink_compliance(sink, stream::iter(vec![input_event]), &YDB_SINK_TAGS).await;

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let count = client.count_rows(&table).await;
    assert_eq!(count, 1, "Expected 1 row with all mapped fields");

    client.drop_table(&table).await;
}

#[tokio::test]
async fn upsert_with_sync_index() {
    trace_init();

    let (config, table, client) = prepare_config().await;

    let create_table_sql = format!(
        r#"CREATE TABLE `{}` (
            id Int64 NOT NULL,
            name Utf8,
            value Int64,
            PRIMARY KEY(id),
            INDEX idx_name GLOBAL ON (name)
        )"#,
        table
    );

    client
        .table_client
        .retry_execute_scheme_query(create_table_sql)
        .await
        .expect("Failed to create table with index");

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let mut event = LogEvent::from("index test");
    event.insert("id", 100_i64);
    event.insert("name", "indexed-user");
    event.insert("value", 42_i64);

    let (input_event, mut receiver) = {
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        (Event::from(event).with_batch_notifier(&batch), receiver)
    };

    run_and_assert_sink_compliance(sink, stream::iter(vec![input_event]), &YDB_SINK_TAGS).await;

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let count = client.count_rows(&table).await;
    assert_eq!(count, 1, "Expected 1 row inserted via transactional UPSERT");

    client.drop_table(&table).await;
}

#[tokio::test]
async fn schema_refresh_on_index_addition() {
    use crate::sinks::ydb::service::{YdbRequest, YdbService};
    use tower::ServiceExt;

    trace_init();

    let (config, table, client) = prepare_config().await;
    let table_client = client.table_client.clone();

    let create_table_sql = format!(
        r#"CREATE TABLE `{}` (
            id Int64 NOT NULL,
            name Utf8,
            value Int64,
            PRIMARY KEY(id)
        )"#,
        table
    );

    table_client
        .retry_execute_scheme_query(create_table_sql)
        .await
        .expect("Failed to create table");

    let initial_schema = table_client
        .describe_table(table.clone())
        .await
        .expect("Failed to describe table");

    let mut service = YdbService::new(
        table_client.clone(),
        table.clone(),
        config.endpoint.clone(),
        initial_schema,
    );

    let mut event1 = LogEvent::from("before index");
    event1.insert("id", 1_i64);
    event1.insert("name", "user1");
    event1.insert("value", 10_i64);

    let request1 = YdbRequest::try_from(vec![Event::from(event1)]).unwrap();
    service.ready().await.unwrap().call(request1).await.unwrap();

    let add_index_sql = format!(
        r#"ALTER TABLE `{}` ADD INDEX idx_name GLOBAL ON (name)"#,
        table
    );

    table_client
        .retry_execute_scheme_query(add_index_sql)
        .await
        .expect("Failed to add index");

    let mut event2 = LogEvent::from("after index");
    event2.insert("id", 2_i64);
    event2.insert("name", "user2");
    event2.insert("value", 20_i64);

    let request2 = YdbRequest::try_from(vec![Event::from(event2)]).unwrap();
    service.ready().await.unwrap().call(request2).await.unwrap();

    let count_query = format!("SELECT COUNT(*) as cnt FROM `{}`", table);
    let count_result = table_client
        .retry_transaction(|mut t| {
            let query = count_query.clone();
            async move {
                let result_set = t.query(Query::new(&query)).await?;
                let value = result_set.into_only_row()?.remove_field_by_name("cnt")?;
                Ok(value)
            }
        })
        .await
        .unwrap();

    let count: u64 = count_result.try_into().unwrap();
    assert_eq!(
        count, 2,
        "Expected 2 rows: 1 via bulk_upsert, 1 via UPSERT after auto-refresh"
    );

    client.drop_table(&table).await;
}
