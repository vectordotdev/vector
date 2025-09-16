use crate::sources::odbc::client::OdbcConfig;
use crate::test_util::components::{run_and_assert_source_compliance, SOURCE_TAGS};
use std::time::Duration;

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
        "#
    );
    let config = toml::from_str::<OdbcConfig>(&config_str);
    assert!(config.is_ok(), "Failed to parse config: {}", config.unwrap_err());
}

#[tokio::test]
async fn scheduled_query_executed() {
    let conn_str = get_conn_str();
    run_and_assert_source_compliance(
        OdbcConfig {
            connection_string: conn_str,
            schedule: Some("*/1 * * * * *".into()),
            statement: Some("SELECT 1".to_string()),
            ..Default::default()
        },
        Duration::from_secs(3),
        &SOURCE_TAGS
    ).await;
}

fn get_conn_str() -> String {
    std::env::var("ODBC_CONN_STRING")
        .expect("Required environment variable 'ODBC_CONN_STRING'")
}