use futures::{future::ready, stream};
use sqlx::{
    mysql::{MySqlConnectOptions, MySqlPoolOptions},
    Executor as _, MySqlPool, Row,
};
use std::collections::HashMap;
use vector_common::sensitive_string::SensitiveString;
use vector_lib::event::{BatchNotifier, BatchStatusReceiver, Event, LogEvent, Value};

use super::*;
use crate::sinks::prelude::TowerRequestConfig;
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::util::{BatchConfig, Compression},
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        random_string, trace_init,
    },
};

// Set up Doris connection information
fn doris_address() -> String {
    std::env::var("DORIS_ADDRESS").unwrap_or_else(|_| "http://10.16.10.6:8630".into())
}

// Extract MySQL connection information from HTTP address
fn extract_mysql_conn_info(http_address: &str) -> (String, u16) {
    // Default MySQL port - user specified as 9630
    let default_port = 9630;

    // Parse HTTP address
    if let Ok(url) = url::Url::parse(http_address) {
        let host = url.host_str().unwrap_or("127.0.0.1").to_string();
        return (host, default_port);
    }

    // If parsing fails, return default values
    ("127.0.0.1".to_string(), default_port)
}

fn make_test_event() -> (Event, BatchStatusReceiver) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let mut event = LogEvent::from("raw log line").with_batch_notifier(&batch);
    event.insert("host", "apache.com");
    event.insert("timestamp", "2025-04-17 00:00:00");
    (event.into(), receiver)
}

// Verify event fields match database row data
fn assert_fields_match(
    event_log: &LogEvent,
    db_row: &HashMap<String, DbValue>,
    fields: &[&str],
    table_name: Option<&str>,
) {
    for field in fields {
        // Get field value from event
        let event_value = event_log.get(*field).cloned().unwrap_or(Value::Null);

        // Get field value from database row
        let db_value = db_row.get(*field).cloned().unwrap_or(DbValue::Null);

        // Convert event value to string
        let event_str = match &event_value {
            Value::Bytes(bytes) => String::from_utf8_lossy(bytes).to_string(),
            other => other.to_string(),
        };

        // Database value already has Display implementation, use directly
        let db_str = db_value.to_string();

        // Build error message
        let error_msg = if let Some(table) = table_name {
            format!("Field '{}' mismatch in table {}", field, table)
        } else {
            format!("Field '{}' mismatch", field)
        };

        // Compare string representations
        assert_eq!(event_str, db_str, "{}", error_msg);
    }
}

#[derive(Clone)]
struct DorisAuth {
    user: String,
    password: String,
}

fn config_auth() -> DorisAuth {
    DorisAuth {
        user: "root".to_string(),
        password: "123456".to_string(),
    }
}

fn default_headers() -> HashMap<String, String> {
    vec![
        ("format".to_string(), "json".to_string()),
        ("strip_outer_array".to_string(), "false".to_string()),
        ("read_json_by_line".to_string(), "true".to_string()),
    ]
    .into_iter()
    .collect()
}

#[tokio::test]
async fn insert_events() {
    trace_init();

    tracing::info!("Starting insert_events test");

    let database = format!("test_db_{}_point", random_string(5).to_lowercase());
    let table = format!("test_table_{}", random_string(5).to_lowercase());

    tracing::info!("Creating test database {} and table {}", database, table);

    // Create Doris client and test table
    let client = DorisTestClient::new(doris_address()).await;
    client.create_database(&database).await;
    client
        .create_table(
            &database,
            &table,
            "host Varchar(100), timestamp String, message String",
        )
        .await;

    tracing::info!("Successfully created database and table");

    // Configure Doris sink
    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = DorisConfig {
        endpoints: vec![doris_address()],
        database: database.clone().try_into().unwrap(),
        table: table.clone().try_into().unwrap(),
        compression: Compression::None,
        auth: Some(crate::http::Auth::Basic {
            user: config_auth().user.clone(),
            password: SensitiveString::from(config_auth().password.clone()),
        }),
        request: TowerRequestConfig {
            retry_attempts: 1,
            ..Default::default()
        },
        batch,
        headers: default_headers(),
        log_request: true,
        ..Default::default()
    };

    tracing::info!("Doris sink configuration: {:?}", config);

    // Build sink
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    tracing::info!("Successfully built sink");

    let (event, _receiver) = make_test_event();
    tracing::info!("Created test event: {:?}", event);

    tracing::info!("Starting sink...");
    // This will wait for sink to completely process all events
    run_and_assert_sink_compliance(sink, stream::once(ready(event.clone())), &SINK_TAGS).await;
    tracing::info!("Sink finished running");

    tracing::info!("Verifying data insertion");
    let row_count = client.count_rows(&database, &table).await;
    assert_eq!(1, row_count, "Table should have exactly 1 row");

    // Verify data content
    let event_log = event.into_log();
    let db_row = client.get_first_row(&database, &table).await;

    // Use helper function to check field matching
    assert_fields_match(&event_log, &db_row, &["host", "timestamp", "message"], None);

    // assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    client.drop_table(&database, &table).await;
    client.drop_database(&database).await;
}

#[tokio::test]
async fn insert_events_with_compression() {
    trace_init();

    tracing::info!("Starting insert_events_with_compression test");

    let database = format!("test_db_{}", random_string(5).to_lowercase());
    let table = format!("test_table_{}", random_string(5).to_lowercase());

    tracing::info!("Creating test database {} and table {}", database, table);

    // Create Doris client and test table
    let client = DorisTestClient::new(doris_address()).await;
    client.create_database(&database).await;
    client
        .create_table(
            &database,
            &table,
            "host Varchar(100), timestamp String, message String",
        )
        .await;

    tracing::info!("Successfully created database and table");

    // Configure Doris sink with GZIP compression
    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = DorisConfig {
        endpoints: vec![doris_address()],
        database: database.clone().try_into().unwrap(),
        table: table.clone().try_into().unwrap(),
        compression: Compression::gzip_default(),
        batch,
        auth: Some(crate::http::Auth::Basic {
            user: config_auth().user.clone(),
            password: SensitiveString::from(config_auth().password.clone()),
        }),
        log_request: true,
        headers: default_headers(),
        ..Default::default()
    };

    tracing::info!(
        "Doris sink configuration (with GZIP compression): {:?}",
        config
    );

    // Build sink
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    tracing::info!("Successfully built sink");

    // Create test event
    let (event, _receiver) = make_test_event();
    tracing::info!("Created test event: {:?}", event);

    // Run sink and verify
    tracing::info!("Starting sink...");
    run_and_assert_sink_compliance(sink, stream::once(ready(event.clone())), &SINK_TAGS).await;
    tracing::info!("Sink finished running");

    tracing::info!("Verifying data insertion");
    let row_count = client.count_rows(&database, &table).await;
    assert_eq!(1, row_count, "Table should have exactly 1 row");

    // Verify data content
    let event_log = event.into_log();
    let db_row = client.get_first_row(&database, &table).await;

    // Use helper function to check field matching
    assert_fields_match(&event_log, &db_row, &["host", "timestamp", "message"], None);

    // assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
    tracing::info!("Cleaning up test resources");
    client.drop_table(&database, &table).await;
    client.drop_database(&database).await;
    tracing::info!("Test completed, resources cleaned up");
}

#[tokio::test]
async fn insert_events_with_templated_table() {
    trace_init();

    tracing::info!("Starting insert_events_with_templated_table test");

    let database = format!("test_db_{}", random_string(5).to_lowercase());
    let table_prefix = format!("test_table_{}", random_string(5).to_lowercase());

    // Create multiple tables, for templated table name test
    let tables = vec![
        format!("{}_{}", table_prefix, "users"),
        format!("{}_{}", table_prefix, "orders"),
    ];

    tracing::info!(
        "Creating test database {} and tables {:?}",
        database,
        tables
    );

    // Create Doris client and test tables
    let client = DorisTestClient::new(doris_address()).await;
    client.create_database(&database).await;

    for table in &tables {
        client
            .create_table(
                &database,
                table,
                "host Varchar(100), timestamp String, message String, table_suffix String",
            )
            .await;
    }

    tracing::info!("Successfully created database and tables");

    // Configure Doris sink with templated table name
    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = DorisConfig {
        endpoints: vec![doris_address()],
        database: database.clone().try_into().unwrap(),
        table: format!("{}_{{{{ table_suffix }}}}", table_prefix)
            .try_into()
            .unwrap(),
        compression: Compression::None,
        auth: Some(crate::http::Auth::Basic {
            user: config_auth().user.clone(),
            password: SensitiveString::from(config_auth().password.clone()),
        }),
        headers: default_headers(),
        log_request: true,
        batch,
        ..Default::default()
    };

    tracing::info!(
        "Doris sink configuration (with templated table name): {:?}",
        config
    );

    // Build sink
    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();
    tracing::info!("Successfully built sink");

    // Create test events with different table name suffixes
    let mut events = Vec::new();
    let mut receivers = Vec::new();

    for suffix in &["users", "orders"] {
        let (batch, receiver) = BatchNotifier::new_with_receiver();
        let mut event = LogEvent::from("raw log line").with_batch_notifier(&batch);
        event.insert("host", "example.com");
        event.insert("timestamp", Value::Null); // Add timestamp field
        event.insert("table_suffix", suffix.to_string());
        events.push(Event::from(event));
        receivers.push((suffix.to_string(), receiver));
        tracing::info!("Created test event, table suffix: {}", suffix);
    }

    // Run sink and verify
    tracing::info!("Starting sink...");
    run_and_assert_sink_compliance(sink, stream::iter(events.clone()), &SINK_TAGS).await;
    tracing::info!("Sink finished running");

    // Verify receiving status - Skip check
    tracing::info!("Skipping status verification, directly verifying data insertion");

    // Verify data insertion into each table
    tracing::info!("Verifying data insertion");
    for (i, table) in tables.iter().enumerate() {
        // Check row count
        let row_count = client.count_rows(&database, table).await;
        assert_eq!(1, row_count, "Table {} should have exactly 1 row", table);

        // Get event and database row
        let event_log = events[i].clone().into_log();
        let db_row = client.get_first_row(&database, table).await;

        // Use helper function to check field matching
        assert_fields_match(&event_log, &db_row, &["host", "table_suffix"], Some(table));

        tracing::info!("Table {} data verification successful", table);
    }

    // Clean up test resources
    tracing::info!("Cleaning up test resources");
    for table in &tables {
        client.drop_table(&database, table).await;
    }
    client.drop_database(&database).await;
    tracing::info!("Test completed, resources cleaned up");
}

// Define an enum type that can represent different types of values
#[derive(Debug, Clone)]
enum DbValue {
    String(String),
    Integer(i64),
    Float(f64),
    Null,
}

impl std::fmt::Display for DbValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbValue::String(s) => write!(f, "{}", s),
            DbValue::Integer(i) => write!(f, "{}", i),
            DbValue::Float(fl) => write!(f, "{}", fl),
            DbValue::Null => write!(f, "null"),
        }
    }
}

#[derive(Clone)]
struct DorisTestClient {
    pool: MySqlPool,
}

impl DorisTestClient {
    async fn new(http_address: String) -> Self {
        let auth = config_auth();
        let (host, port) = extract_mysql_conn_info(&http_address);

        tracing::info!(
            "Connected to Doris MySQL interface: {}:{} User: {}",
            host,
            port,
            auth.user
        );

        // Configure MySQL connection parameters - For Doris specifically adjusted
        let connect_options = MySqlConnectOptions::new()
            .host(&host)
            .port(port)
            .username(&auth.user)
            .password(&auth.password)
            .no_engine_substitution(false)
            .pipes_as_concat(false)
            .ssl_mode(sqlx::mysql::MySqlSslMode::Disabled);

        // Create connection pool - More conservative connection settings
        let pool = match MySqlPoolOptions::new()
            .max_connections(1) // Limit to single connection
            .idle_timeout(std::time::Duration::from_secs(10))
            .connect_with(connect_options)
            .await
        {
            Ok(pool) => {
                tracing::info!("Successfully created MySQL connection pool");
                pool
            }
            Err(e) => {
                tracing::error!("Failed to create MySQL connection pool: {}", e);
                panic!("Failed to create MySQL connection pool: {}", e);
            }
        };

        DorisTestClient { pool }
    }

    async fn execute_query(&self, query: &str) {
        tracing::info!("Executing SQL query: {}", query);

        // Fully use non-prepare text protocol
        match self.pool.execute(query).await {
            Ok(result) => {
                tracing::info!(
                    "SQL query execution successful: {} - Affected rows: {}",
                    query,
                    result.rows_affected()
                );
            }
            Err(e) => {
                // For some errors, if the database or table already exists, we can ignore them
                if query.starts_with("CREATE DATABASE") && e.to_string().contains("already exists")
                {
                    tracing::warn!("Database may already exist, continuing execution: {}", e);
                    return;
                } else if query.starts_with("CREATE TABLE")
                    && e.to_string().contains("already exists")
                {
                    tracing::warn!("Table may already exist, continuing execution: {}", e);
                    return;
                } else {
                    panic!("SQL query execution failed: {} - {}", query, e);
                }
            }
        };
    }

    // Simplified method to create database, directly use execute_query
    async fn create_database(&self, database: &str) {
        let query = format!("CREATE DATABASE IF NOT EXISTS {}", database);
        self.execute_query(&query).await;
    }

    // Simplified method to create table, directly use execute_query
    async fn create_table(&self, database: &str, table: &str, schema: &str) {
        let query = format!(
            "CREATE TABLE IF NOT EXISTS {}.{} ({}) ENGINE=OLAP 
             DISTRIBUTED BY HASH(`host`) BUCKETS 1 
             PROPERTIES(\"replication_num\" = \"1\")",
            database, table, schema
        );
        self.execute_query(&query).await;
    }

    // Simplified method to drop table
    async fn drop_table(&self, database: &str, table: &str) {
        let query = format!("DROP TABLE IF EXISTS {}.{}", database, table);
        self.execute_query(&query).await;
    }

    // Simplified method to drop database
    async fn drop_database(&self, database: &str) {
        let query = format!("DROP DATABASE IF EXISTS {}", database);
        self.execute_query(&query).await;
    }

    async fn count_rows(&self, database: &str, table: &str) -> i64 {
        let query = format!("SELECT COUNT(*) FROM {}.{}", database, table);
        tracing::info!("Counting rows: {}", query);

        // Use fetch_one and get directly to get results, avoid using query_scalar
        let row = match self.pool.fetch_one(query.as_str()).await {
            Ok(row) => row,
            Err(e) => {
                panic!("Counting rows failed: {} - {}", query, e);
            }
        };

        // Get the value of the first column as count
        let count: i64 = row.get(0);
        tracing::info!("Count result: {} rows", count);
        count
    }

    // Modify get_first_row method, return HashMap<String, DbValue>
    async fn get_first_row(&self, database: &str, table: &str) -> HashMap<String, DbValue> {
        let query = format!("SELECT * FROM {}.{} LIMIT 1", database, table);
        tracing::info!("Getting first row data: {}", query);

        // Get column names
        let columns = self.get_column_names(database, table).await;

        // Get data - Directly use Executor interface
        let row = match self.pool.fetch_one(query.as_str()).await {
            Ok(row) => row,
            Err(e) => {
                panic!("Failed to get first row data: {} - {}", query, e);
            }
        };

        // Build result
        let mut result = HashMap::new();
        for (i, column) in columns.iter().enumerate() {
            // Try different types one by one, directly store original values
            if let Ok(value) = row.try_get::<Option<String>, _>(i) {
                result.insert(
                    column.clone(),
                    match value {
                        Some(s) => DbValue::String(s),
                        None => DbValue::Null,
                    },
                );
            } else if let Ok(value) = row.try_get::<Option<i64>, _>(i) {
                result.insert(
                    column.clone(),
                    match value {
                        Some(n) => DbValue::Integer(n),
                        None => DbValue::Null,
                    },
                );
            } else if let Ok(value) = row.try_get::<Option<f64>, _>(i) {
                result.insert(
                    column.clone(),
                    match value {
                        Some(f) => DbValue::Float(f),
                        None => DbValue::Null,
                    },
                );
            } else {
                // Default to Null
                result.insert(column.clone(), DbValue::Null);
            }
        }

        tracing::info!("Getting first row data successful");
        result
    }

    async fn get_column_names(&self, database: &str, table: &str) -> Vec<String> {
        // Use INFORMATION_SCHEMA.COLUMNS to get column names
        let query = format!(
            "SELECT COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' ORDER BY ORDINAL_POSITION",
            database, table
        );

        // Use Executor interface directly to execute, avoid precompiled
        match self.pool.fetch_all(query.as_str()).await {
            Ok(rows) => rows.iter().map(|row| row.get::<String, _>(0)).collect(),
            Err(e) => {
                tracing::warn!("Failed to get column names: {} - {}", query, e);
                // If column names cannot be obtained, return empty list
                Vec::new()
            }
        }
    }
}
