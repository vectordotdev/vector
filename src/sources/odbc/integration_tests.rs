use crate::config::{SourceConfig, SourceContext};
use crate::sources::odbc::client::OdbcConfig;
use crate::test_util::components::{assert_source_compliance, SOURCE_TAGS};
use crate::SourceSender;

#[tokio::test]
async fn healthcheck_passed() {
    let config_str = format!(
        r#"
            connection_string = "driver={{MySQL ODBC 8.0 ANSI Driver}};server=localhost;port=3306;database=vector_db;uid=vector;pwd=vector;"
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
    assert_source_compliance(&SOURCE_TAGS, async {
        let config = OdbcConfig {
            connection_string: "driver={MySQL ODBC 8.0 ANSI Driver};server=localhost;port=3306;database=vector_db;uid=vector;pwd=vector;".to_string(),
            schedule: Some("*/1 * * * * *".into()),
            statement: Some("SELECT 1".to_string()),
            ..Default::default()
        };
        let (sender, _logs_output) = SourceSender::new_test();
        let server = config
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();
        tokio::spawn(server);
    }).await;
}