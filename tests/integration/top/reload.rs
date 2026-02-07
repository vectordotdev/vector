//! Tests for config reloading and metric updates
//!
//! Verifies that `vector top` reflects changes after Vector reloads
//! its configuration.

use std::process::Command;

use assert_cmd::prelude::*;
use indoc::{formatdoc, indoc};
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use tokio::time::sleep;
use vector_lib::api_client::gql::{ComponentsQueryExt, components_query};

use super::harness::{
    API_READY_TIMEOUT, RELOAD_TIME, STARTUP_TIME, TestHarness, create_config_file,
    create_data_directory, overwrite_config_file, wait_for_api_ready,
};

#[tokio::test]
async fn config_reload_updates_components() {
    // Initial config: 1 source -> 1 sink
    let mut runner = TestHarness::new(indoc! {"
        sources:
          demo1:
            type: demo_logs
            format: json
            interval: 0.1

        sinks:
          blackhole1:
            type: blackhole
            inputs: ['demo1']
    "})
    .await
    .expect("Failed to start Vector");

    // Verify initial components
    let data = runner.query_components().await.expect("Failed to query");

    let component_ids: Vec<String> = data
        .components
        .edges
        .iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(component_ids.contains(&"demo1".to_string()));
    assert!(component_ids.contains(&"blackhole1".to_string()));
    assert_eq!(component_ids.len(), 2);

    // RELOAD 1: Add components
    runner
        .reload_with_config(indoc! {"
            sources:
              demo1:
                type: demo_logs
                format: json
                interval: 0.1

              demo2:
                type: demo_logs
                format: json
                interval: 0.1

            sinks:
              blackhole1:
                type: blackhole
                inputs: ['demo1']

              blackhole2:
                type: blackhole
                inputs: ['demo2']
        "})
        .await
        .expect("Failed to reload config");

    // Verify additions
    assert!(runner.check_running(), "Vector should still be running");

    let data = runner.query_components().await.expect("Failed to query");

    let component_ids: Vec<String> = data
        .components
        .edges
        .iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(component_ids.contains(&"demo1".to_string()));
    assert!(component_ids.contains(&"demo2".to_string()));
    assert!(component_ids.contains(&"blackhole1".to_string()));
    assert!(component_ids.contains(&"blackhole2".to_string()));
    assert_eq!(component_ids.len(), 4);

    // Verify new components are processing events
    runner
        .wait_for_events("demo2", 10)
        .await
        .expect("New source should process events");

    // RELOAD 2: Remove components
    runner
        .reload_with_config(indoc! {"
            sources:
              demo1:
                type: demo_logs
                format: json
                interval: 0.1

            sinks:
              blackhole1:
                type: blackhole
                inputs: ['demo1']
        "})
        .await
        .expect("Failed to reload config");

    // Verify removals
    let data = runner.query_components().await.expect("Failed to query");

    let component_ids: Vec<String> = data
        .components
        .edges
        .iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(component_ids.contains(&"demo1".to_string()));
    assert!(!component_ids.contains(&"demo2".to_string()));
    assert!(component_ids.contains(&"blackhole1".to_string()));
    assert!(!component_ids.contains(&"blackhole2".to_string()));
    assert_eq!(component_ids.len(), 2);
}

#[tokio::test]
async fn watch_mode_auto_reloads() {
    // Get an available port (watch mode needs manual process management)
    let api_port = {
        let runner = TestHarness::new(indoc! {"
            sources:
              dummy_src:
                type: demo_logs
                format: json

            sinks:
              dummy_sink:
                type: blackhole
                inputs: ['dummy_src']
        "})
        .await
        .unwrap();
        let port = runner.api_port();
        drop(runner); // Free up the port
        port
    };

    let initial_config = formatdoc! {"
        api:
          enabled: true
          address: \"127.0.0.1:{api_port}\"

        sources:
          demo:
            type: demo_logs
            format: json
            interval: 0.1

        sinks:
          blackhole:
            type: blackhole
            inputs: ['demo']
    "};

    let config_path = create_config_file(&initial_config);

    // Start with watch flag
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c")
        .arg(&config_path)
        .arg("-w") // Watch mode - auto reload on file change
        .env("VECTOR_DATA_DIR", create_data_directory());

    let mut vector = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start vector");

    sleep(STARTUP_TIME).await;

    let client = vector_lib::api_client::Client::new(
        format!("http://127.0.0.1:{api_port}/graphql")
            .parse()
            .unwrap(),
    );

    // Wait for API
    wait_for_api_ready(&client, API_READY_TIMEOUT)
        .await
        .expect("API did not become ready");

    // Initial state: 1 source + 1 sink
    let data: components_query::ResponseData =
        client.components_query(100).await.unwrap().data.unwrap();
    let initial_count = data.components.edges.len();
    assert_eq!(initial_count, 2);

    // Modify config - Vector should auto-reload
    let new_config = formatdoc! {"
        api:
          enabled: true
          address: \"127.0.0.1:{api_port}\"

        sources:
          demo:
            type: demo_logs
            format: json
            interval: 0.1

          demo2:
            type: demo_logs
            format: json
            interval: 0.1

        sinks:
          blackhole:
            type: blackhole
            inputs: ['demo', 'demo2']
    "};

    overwrite_config_file(&config_path, &new_config);

    // Wait for auto-reload (no SIGHUP needed!)
    sleep(RELOAD_TIME).await;

    // Verify new component appeared
    let data: components_query::ResponseData =
        client.components_query(100).await.unwrap().data.unwrap();

    let component_ids: Vec<String> = data
        .components
        .edges
        .iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(component_ids.contains(&"demo2".to_string()));
    assert_eq!(component_ids.len(), 3);

    // Manual cleanup
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGTERM).ok();
    vector.wait().ok();
}
