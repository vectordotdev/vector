#![cfg(feature = "enterprise-tests")]

use std::{env, path::PathBuf, thread};

use http::StatusCode;

use vector::{
    app::Application,
    cli::{Color, LogFormat, Opts, RootOpts},
    config::enterprise::{DATADOG_API_KEY_ENV_VAR_FULL, DATADOG_API_KEY_ENV_VAR_SHORT},
};
use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

const ENDPOINT_CONFIG_ENV_VAR: &'static str = "MOCK_SERVER_ENDPOINT";

/// This mocked server will reply with the configured status code 3 times
/// before falling back to a 200 OK
async fn build_test_server_error_and_recover(status_code: StatusCode) -> MockServer {
    let mock_server = MockServer::start().await;

    Mock::given(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(status_code))
        .up_to_n_times(3)
        .with_priority(1)
        .mount(&mock_server)
        .await;

    Mock::given(matchers::method("POST"))
        .respond_with(ResponseTemplate::new(StatusCode::OK))
        .with_priority(2)
        .mount(&mock_server)
        .await;

    mock_server
}

fn get_root_opts(config_path: PathBuf) -> RootOpts {
    RootOpts {
        config_paths: vec![config_path],
        config_dirs: vec![],
        config_paths_toml: vec![],
        config_paths_json: vec![],
        config_paths_yaml: vec![],
        require_healthy: None,
        threads: None,
        verbose: 0,
        quiet: 3,
        internal_log_rate_limit: 10,
        log_format: LogFormat::Text,
        color: Color::Auto,
        watch_config: false,
    }
}

/// This test asserts that configuration reporting errors do NOT impact the
/// rest of Vector starting and running.
///
/// In general, Vector should continue operating even in the event that the
/// enterprise API is down/having issues. Do not modify this behavior
/// without prior approval.
#[tokio::test]
async fn vector_continues_on_reporting_error() {
    vector::metrics::init_test();

    let server = build_test_server_error_and_recover(StatusCode::NOT_IMPLEMENTED).await;
    let endpoint = server.uri();

    env::set_var(ENDPOINT_CONFIG_ENV_VAR, endpoint);
    env::set_var(DATADOG_API_KEY_ENV_VAR_SHORT, "api_key");
    env::set_var("DD_CONFIGURATION_KEY", "configuration_key");
    let config_file = PathBuf::from(format!(
        "{}/tests/data/enterprise/base.toml",
        env!("CARGO_MANIFEST_DIR")
    ));

    let opts = Opts {
        root: get_root_opts(config_file),
        sub_command: None,
    };

    // Spawn a separate thread to avoid nested async runtime errors
    let vector_continued = thread::spawn(|| {
        // Configuration reporting is guaranteed to fail here due to API
        // server issues. However, the app should still start up and run.
        Application::prepare_from_opts(opts).map_or(false, |app| {
            // Finish running the topology to avoid error logs
            app.run();
            true
        })
    })
    .join()
    .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    assert!(!server.received_requests().await.unwrap().is_empty());
    assert!(vector_continued);
}

#[tokio::test]
async fn vector_does_not_start_with_enterprise_misconfigured() {
    vector::metrics::init_test();

    let server = build_test_server_error_and_recover(StatusCode::NOT_IMPLEMENTED).await;
    let endpoint = server.uri();

    // Control for API key environment variables which are an alternative
    // way of passing in an API key, outside of a configuration
    env::remove_var(DATADOG_API_KEY_ENV_VAR_FULL);
    env::remove_var(DATADOG_API_KEY_ENV_VAR_SHORT);
    env::set_var(ENDPOINT_CONFIG_ENV_VAR, endpoint);
    let config_file = PathBuf::from(format!(
        "{}/tests/data/enterprise/missing_api_key.toml",
        env!("CARGO_MANIFEST_DIR")
    ));

    let opts = Opts {
        root: get_root_opts(config_file),
        sub_command: None,
    };

    let vector_failed_to_start = thread::spawn(|| {
        // With [enterprise] configured but no API key, starting the app
        // should fail
        Application::prepare_from_opts(opts).is_err()
    })
    .join()
    .unwrap();

    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(vector_failed_to_start);
}
