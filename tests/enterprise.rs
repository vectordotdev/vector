#![cfg(feature = "enterprise-tests")]

use std::{io::Write, path::PathBuf, str::FromStr, thread};

use http::StatusCode;
use indoc::formatdoc;

use vector::{
    app::Application,
    cli::{Color, LogFormat, Opts, RootOpts},
};
use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

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

fn get_vector_config_file(config: impl Into<String>) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    let _ = writeln!(file, "{}", config.into());
    file
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
        log_format: LogFormat::from_str("text").unwrap(),
        color: Color::from_str("auto").unwrap(),
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
    let _ = vector::metrics::init_test();

    let server = build_test_server_error_and_recover(StatusCode::NOT_IMPLEMENTED).await;
    let endpoint = server.uri();

    let vector_config = formatdoc! {r#"
        [enterprise]
        application_key = "application_key"
        api_key = "api_key"
        configuration_key = "configuration_key"
        endpoint = "{endpoint}"
        max_retries = 1

        [sources.in]
        type = "demo_logs"
        format = "syslog"
        count = 3

        [sinks.out]
        type = "blackhole"
        inputs = ["*"]
    "#, endpoint=endpoint};

    let config_file = get_vector_config_file(vector_config);

    let opts = Opts {
        root: get_root_opts(config_file.path().to_path_buf()),
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

    assert!(!server.received_requests().await.unwrap().is_empty());
    assert!(vector_continued);
}

#[tokio::test]
async fn vector_does_not_start_with_enterprise_misconfigured() {
    let _ = vector::metrics::init_test();

    let server = build_test_server_error_and_recover(StatusCode::NOT_IMPLEMENTED).await;
    let endpoint = server.uri();

    let vector_config = formatdoc! {r#"
        [enterprise]
        application_key = "application_key"
        configuration_key = "configuration_key"
        endpoint = "{endpoint}"
        max_retries = 1

        [sources.in]
        type = "demo_logs"
        format = "syslog"
        count = 1
        interval = 0.0

        [sinks.out]
        type = "blackhole"
        inputs = ["*"]
    "#, endpoint=endpoint};

    let config_file = get_vector_config_file(vector_config);

    let opts = Opts {
        root: get_root_opts(config_file.path().to_path_buf()),
        sub_command: None,
    };

    let vector_failed_to_start = thread::spawn(|| {
        // With [enterprise] configured but no API key, starting the app
        // should fail
        Application::prepare_from_opts(opts).is_err()
    })
    .join()
    .unwrap();

    assert!(vector_failed_to_start);
}
