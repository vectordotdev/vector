use crate::event::Event;
use crate::sources::odbc::client::OdbcConfig;
use crate::test_util::components::{run_and_assert_source_compliance, SOURCE_TAGS};
use odbc_api::ConnectionOptions;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::time::Duration;

fn get_conn_str() -> String {
    std::env::var("ODBC_CONN_STRING").expect("Required environment variable 'ODBC_CONN_STRING'")
}

#[tokio::test]
async fn parse_odbc_config() {
    let conn_str = get_conn_str();
    let config_str = format!(
        r#"
            connection_string = "{conn_str}"
            statement = "SELECT * FROM odbc_table WHERE id > ? LIMIT 1;"
            schedule = "*/5 * * * * *"
            schedule_timezone = "UTC"
            last_run_metadata_path = "odbc_tracking.json"
            tracking_columns = ["id", "name", "datetime"]
            statement_init_params = {{ id = "0", name = "test" }}
            iterations = 1
        "#
    );
    let config = toml::from_str::<OdbcConfig>(&config_str);
    assert!(
        config.is_ok(),
        "Failed to parse config: {}",
        config.unwrap_err()
    );
}

#[tokio::test]
async fn scheduled_query_executed() {
    let conn_str = get_conn_str();
    run_and_assert_source_compliance(
        OdbcConfig {
            connection_string: conn_str,
            schedule: Some("*/1 * * * * *".into()),
            statement: Some("SELECT 1".to_string()),
            iterations: Some(1),
            ..Default::default()
        },
        Duration::from_secs(3),
        &SOURCE_TAGS,
    )
    .await;
}

#[tokio::test]
async fn query_executed_with_init_params() {
    const LAST_RUN_METADATA_PATH: &str = "odbc_tracking-integration-tests.json";

    let conn_str = get_conn_str();
    let env = odbc_api::Environment::new().unwrap();
    let conn = env
        .connect_with_connection_string(&conn_str, ConnectionOptions::default())
        .unwrap();
    let _ = conn
        .execute("DROP TABLE IF EXISTS odbc_table;", (), Some(3))
        .unwrap();
    let _ = conn
        .execute(
            r#"
CREATE TABLE odbc_table
(
    id int auto_increment primary key,
    name varchar(255) null,
    datetime datetime null
);
    "#,
            (),
            Some(3),
        )
        .unwrap();
    let _ = conn
        .execute(
            r#"
INSERT INTO odbc_table (name, datetime) VALUES
('test1', now()),
('test2', now()),
('test3', now()),
('test4', now()),
('test5', now());
    "#,
            (),
            Some(3),
        )
        .unwrap();
    let params = BTreeMap::from([("id".to_string(), "0".to_string())]);

    let _ = fs::remove_file(LAST_RUN_METADATA_PATH);

    let events = run_and_assert_source_compliance(
        OdbcConfig {
            connection_string: conn_str,
            schedule: Some("*/1 * * * * *".into()),
            statement: Some("SELECT * FROM odbc_table WHERE id > ? LIMIT 1;".to_string()),
            statement_init_params: Some(params),
            tracking_columns: Some(vec!["id".to_string()]),
            last_run_metadata_path: Some(LAST_RUN_METADATA_PATH.to_string()),
            iterations: Some(5),
            ..Default::default()
        },
        Duration::from_secs(10),
        &SOURCE_TAGS,
    )
    .await;

    println!("{}", serde_json::to_string_pretty(&events).unwrap());
    assert_eq!(
        get_value_from_event(&events[0], "name"),
        Some("test1".into())
    );
    assert_eq!(
        get_value_from_event(&events[1], "name"),
        Some("test2".into())
    );
    assert_eq!(
        get_value_from_event(&events[2], "name"),
        Some("test3".into())
    );
    assert_eq!(
        get_value_from_event(&events[3], "name"),
        Some("test4".into())
    );
    assert_eq!(
        get_value_from_event(&events[4], "name"),
        Some("test5".into())
    );
}

#[tokio::test]
async fn query_executed_with_filepath() {
    const CONNECTION_STRING_FILE_PATH: &str = "odbc_connection_string.txt";
    const STATEMENT_FILE_PATH: &str = "odbc_statement.sql";
    const LAST_RUN_METADATA_PATH: &str = "odbc_tracking-integration-tests.json";

    let conn_str = get_conn_str();
    let env = odbc_api::Environment::new().unwrap();
    let conn = env
        .connect_with_connection_string(&conn_str, ConnectionOptions::default())
        .unwrap();
    let _ = conn
        .execute("DROP TABLE IF EXISTS odbc_table;", (), Some(3))
        .unwrap();
    let _ = conn
        .execute(
            r#"
CREATE TABLE odbc_table
(
    id int auto_increment primary key,
    name varchar(255) null,
    datetime datetime null
);
    "#,
            (),
            Some(3),
        )
        .unwrap();
    let _ = conn
        .execute(
            r#"
INSERT INTO odbc_table (name, datetime) VALUES
('test1', now()),
('test2', now()),
('test3', now()),
('test4', now()),
('test5', now());
    "#,
            (),
            Some(3),
        )
        .unwrap();
    let params = BTreeMap::from([("id".to_string(), "0".to_string())]);

    let _ = fs::write(CONNECTION_STRING_FILE_PATH, conn_str).unwrap();
    let _ = fs::write(
        STATEMENT_FILE_PATH,
        "SELECT * FROM odbc_table WHERE id > ? LIMIT 1;",
    )
    .unwrap();
    let _ = fs::remove_file(LAST_RUN_METADATA_PATH);

    let events = run_and_assert_source_compliance(
        OdbcConfig {
            connection_string_filepath: Some(CONNECTION_STRING_FILE_PATH.to_string()),
            schedule: Some("*/1 * * * * *".into()),
            statement_filepath: Some(STATEMENT_FILE_PATH.to_string()),
            statement_init_params: Some(params),
            tracking_columns: Some(vec!["id".to_string()]),
            last_run_metadata_path: Some(LAST_RUN_METADATA_PATH.to_string()),
            iterations: Some(5),
            ..Default::default()
        },
        Duration::from_secs(10),
        &SOURCE_TAGS,
    )
    .await;

    println!("{}", serde_json::to_string_pretty(&events).unwrap());
    assert_eq!(
        get_value_from_event(&events[0], "name"),
        Some("test1".into())
    );
    assert_eq!(
        get_value_from_event(&events[1], "name"),
        Some("test2".into())
    );
    assert_eq!(
        get_value_from_event(&events[2], "name"),
        Some("test3".into())
    );
    assert_eq!(
        get_value_from_event(&events[3], "name"),
        Some("test4".into())
    );
    assert_eq!(
        get_value_from_event(&events[4], "name"),
        Some("test5".into())
    );
}

fn get_value_from_event<'a>(event: &'a Event, key: &str) -> Option<Cow<'a, str>> {
    let log = event.as_log();
    let msg = log.get_message().unwrap();
    let arr_msg = msg.as_array_unwrap();
    let value = arr_msg[0].get(key);
    value?.as_str()
}
