use futures::{future::ready, stream};
use http::Uri;
use sqlx::{
    mysql::MySqlConnectOptions, ConnectOptions, Connection, Executor as _, MySqlConnection, Row,
};
use std::collections::HashMap;
// use vector_common::finalization::BatchStatus;
use vector_common::sensitive_string::SensitiveString;
use vector_lib::event::{BatchNotifier, BatchStatusReceiver, Event, LogEvent, Value};

use super::*;
use crate::{
    config::{SinkConfig, SinkContext},
    sinks::util::BatchConfig,
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        random_string, trace_init,
    },
};

fn doris_mysql_address_port() -> (String, u16) {
    let host_port = doris_address();
    let uri = host_port.parse::<Uri>().expect("invalid uri");
    let host = uri.host().unwrap_or("localhost").to_string();
    (host, 9030)
}

// Set up Doris connection information
fn doris_address() -> String {
    std::env::var("DORIS_ADDRESS").unwrap_or_else(|_| "http://localhost:8030".into())
}

fn make_event() -> (Event, BatchStatusReceiver) {
    let (batch, receiver) = BatchNotifier::new_with_receiver();
    let mut event = LogEvent::from("raw log line").with_batch_notifier(&batch);
    event.insert("host", "example.com");
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

    let database = format!("test_db_{}_point", random_string(5).to_lowercase());
    let table = format!("test_table_{}", random_string(5).to_lowercase());

    let client = DorisTestClient::new(doris_mysql_address_port()).await;
    tracing::info!("DorisTestClient created successfully, creating database...");

    client.create_database(&database).await;

    client
        .create_table(&database, &table, "host Varchar(100), message String")
        .await;

    let mut batch = BatchConfig::default();
    batch.max_events = Some(1);

    let config = DorisConfig {
        endpoints: vec![doris_address()],
        database: database.clone().try_into().unwrap(),
        table: table.clone().try_into().unwrap(),
        label_prefix: "vector_test".to_string(),
        line_delimiter: "".to_string(),
        log_request: true,
        log_progress_interval: 10,
        headers: default_headers(),
        batch: batch,
        buffer_bound: 1,
        auth: Some(crate::http::Auth::Basic {
            user: config_auth().user.clone(),
            password: SensitiveString::from(config_auth().password.clone()),
        }),
        request: Default::default(),
        ..Default::default()
    };

    let (sink, _hc) = config.build(SinkContext::default()).await.unwrap();

    let (input_event, _rc) = make_event();

    run_and_assert_sink_compliance(sink, stream::once(ready(input_event.clone())), &SINK_TAGS)
        .await;

    // assert_eq!(rc.try_recv(), Ok(BatchStatus::Delivered));

    let row_count = client.count_rows(&database, &table).await;
    assert_eq!(1, row_count);

    let db_row = client.get_first_row(&database, &table).await;

    // Use helper function to check field matching
    assert_fields_match(input_event.as_log(), &db_row, &["host", "message"], None);

    client.drop_table(&database, &table).await;
    client.drop_database(&database).await;
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
    connect_options: MySqlConnectOptions,
}

impl DorisTestClient {
    async fn new(query_address_port: (String, u16)) -> Self {
        let auth = config_auth();
        let (host, port) = query_address_port;

        tracing::info!(
            "Connecting to Doris MySQL interface: {}:{} User: {}",
            host,
            port,
            auth.user
        );

        // Configure MySQL connection parameters - For Doris specifically adjusted
        // Disable these options to not send `SET @@sql_mode=CONCAT(@@sql_mode, {})` which is not supported on Doris.
        let connect_options = MySqlConnectOptions::new()
            .host(&host)
            .port(port)
            .username(&auth.user)
            .password(&auth.password)
            .no_engine_substitution(false) // Keep false to avoid SET statement
            .pipes_as_concat(false) // Keep false to avoid SET statement
            .ssl_mode(sqlx::mysql::MySqlSslMode::Disabled)
            .disable_statement_logging();

        tracing::info!("DorisTestClient initialized successfully");
        DorisTestClient { connect_options }
    }

    /// Create a new database connection
    async fn create_connection(&self) -> MySqlConnection {
        MySqlConnection::connect_with(&self.connect_options)
            .await
            .unwrap_or_else(|e| panic!("Failed to connect to database: {}", e))
    }

    /// Execute a query that doesn't return data (DDL/DML operations)
    async fn execute_query(&self, query: &str) {
        tracing::info!("Executing SQL query: {}", query);

        let mut conn = self.create_connection().await;

        match conn.execute(query).await {
            Ok(result) => {
                tracing::info!(
                    "SQL query execution successful: {} - Affected rows: {}",
                    query,
                    result.rows_affected()
                );
            }
            Err(e) => {
                // Handle specific ignorable errors
                if self.is_ignorable_error(query, &e) {
                    tracing::warn!("Ignoring expected error for query '{}': {}", query, e);
                    return;
                }
                panic!("SQL query execution failed: {} - {}", query, e);
            }
        }
        // Connection is automatically closed when it goes out of scope
    }

    /// Check if an error can be safely ignored
    fn is_ignorable_error(&self, query: &str, error: &sqlx::Error) -> bool {
        let error_str = error.to_string();
        (query.starts_with("CREATE DATABASE") && error_str.contains("already exists"))
            || (query.starts_with("CREATE TABLE") && error_str.contains("already exists"))
    }

    /// Execute a query that returns a single row
    async fn fetch_one_query(&self, query: &str, operation_name: &str) -> sqlx::mysql::MySqlRow {
        tracing::info!("{}: {}", operation_name, query);

        let mut conn = self.create_connection().await;

        let result = conn
            .fetch_one(query)
            .await
            .unwrap_or_else(|e| panic!("{} failed: {} - {}", operation_name, query, e));

        // Connection is automatically closed when it goes out of scope
        result
    }

    /// Execute a query that returns multiple rows
    async fn fetch_all_query(
        &self,
        query: &str,
        operation_name: &str,
    ) -> Vec<sqlx::mysql::MySqlRow> {
        let mut conn = self.create_connection().await;

        let result = conn.fetch_all(query).await.unwrap_or_else(|e| {
            tracing::warn!("{} failed: {} - {}", operation_name, query, e);
            Vec::new()
        });

        // Connection is automatically closed when it goes out of scope
        result
    }

    /// Create database using the common execute pattern
    async fn create_database(&self, database: &str) {
        let query = format!("CREATE DATABASE IF NOT EXISTS {}", database);
        self.execute_query(&query).await;
    }

    /// Create table using the common execute pattern
    async fn create_table(&self, database: &str, table: &str, schema: &str) {
        let query = format!(
            "CREATE TABLE IF NOT EXISTS {}.{} ({}) ENGINE=OLAP
             DISTRIBUTED BY HASH(`host`) BUCKETS 1
             PROPERTIES(\"replication_num\" = \"1\")",
            database, table, schema
        );
        self.execute_query(&query).await;
    }

    /// Drop table using the common execute pattern
    async fn drop_table(&self, database: &str, table: &str) {
        let query = format!("DROP TABLE IF EXISTS {}.{}", database, table);
        self.execute_query(&query).await;
    }

    /// Drop database using the common execute pattern
    async fn drop_database(&self, database: &str) {
        let query = format!("DROP DATABASE IF EXISTS {}", database);
        self.execute_query(&query).await;
    }

    /// Count rows using the common fetch_one pattern
    async fn count_rows(&self, database: &str, table: &str) -> i64 {
        let query = format!("SELECT COUNT(*) FROM {}.{}", database, table);
        let row = self.fetch_one_query(&query, "Counting rows").await;

        let count: i64 = row.get(0);
        tracing::info!("Count result: {} rows", count);
        count
    }

    /// Get column names using the common fetch_all pattern
    async fn get_column_names(&self, database: &str, table: &str) -> Vec<String> {
        let query = format!(
            "SELECT COLUMN_NAME FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = '{}' AND TABLE_NAME = '{}' ORDER BY ORDINAL_POSITION",
            database, table
        );

        let rows = self.fetch_all_query(&query, "Getting column names").await;
        rows.iter().map(|row| row.get::<String, _>(0)).collect()
    }

    /// Convert a database row value to DbValue enum
    fn extract_db_value(row: &sqlx::mysql::MySqlRow, column_index: usize) -> DbValue {
        // Try different types in order of preference
        if let Ok(value) = row.try_get::<Option<String>, _>(column_index) {
            match value {
                Some(s) => DbValue::String(s),
                None => DbValue::Null,
            }
        } else if let Ok(value) = row.try_get::<Option<i64>, _>(column_index) {
            match value {
                Some(n) => DbValue::Integer(n),
                None => DbValue::Null,
            }
        } else if let Ok(value) = row.try_get::<Option<f64>, _>(column_index) {
            match value {
                Some(f) => DbValue::Float(f),
                None => DbValue::Null,
            }
        } else {
            DbValue::Null
        }
    }

    /// Get first row data using the refactored helper methods
    async fn get_first_row(&self, database: &str, table: &str) -> HashMap<String, DbValue> {
        let query = format!("SELECT * FROM {}.{} LIMIT 1", database, table);

        // Get column names and row data using helper methods
        let columns = self.get_column_names(database, table).await;
        let row = self.fetch_one_query(&query, "Getting first row data").await;

        // Build result using the helper method
        let mut result = HashMap::new();
        for (i, column) in columns.iter().enumerate() {
            let db_value = Self::extract_db_value(&row, i);
            result.insert(column.clone(), db_value);
        }

        tracing::info!("Getting first row data successful");
        result
    }
}
