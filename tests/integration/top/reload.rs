//! Tests for config reloading and metric updates
//!
//! Verifies that `vector top` reflects changes after Vector reloads
//! its configuration.

use indoc::indoc;
use super::*;
use tokio::time::sleep;
use std::process::Command;
use assert_cmd::prelude::*;
use nix::{sys::signal::{Signal, kill}, unistd::Pid};
use vector_lib::api_client::gql::ComponentsQueryExt;

#[tokio::test]
async fn config_reload_updates_components() {
    let api_port = find_available_port();

    // Initial config: 1 source -> 1 sink
    let initial_config = build_config_with_api(
        api_port,
        indoc! {"
            sources:
              demo1:
                type: demo_logs
                format: json
                interval: 0.1

            sinks:
              blackhole1:
                type: blackhole
                inputs: ['demo1']
        "},
    );

    let config_path = create_config_file(&initial_config);

    // Start Vector
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c")
       .arg(&config_path)
       .env("VECTOR_DATA_DIR", create_data_directory());

    let mut vector = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start vector");

    sleep(STARTUP_TIME).await;

    let client = create_client(api_port);

    wait_for_api_ready(&client, API_READY_TIMEOUT)
        .await
        .expect("API never became ready");

    // Verify initial components
    let data: components_query::ResponseData = client
        .components_query(100)
        .await
        .unwrap()
        .data
        .unwrap();

    let component_ids: Vec<String> = data.components.edges.iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(component_ids.contains(&"demo1".to_string()));
    assert!(component_ids.contains(&"blackhole1".to_string()));
    assert_eq!(component_ids.len(), 2);

    // RELOAD 1: Add components
    let config_with_additions = build_config_with_api(
        api_port,
        indoc! {"
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
        "},
    );

    overwrite_config_file(&config_path, &config_with_additions);
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGHUP).unwrap();
    sleep(RELOAD_TIME).await;

    // Verify additions
    assert_eq!(None, vector.try_wait().unwrap(), "Vector should still be running");

    let data: components_query::ResponseData = client
        .components_query(100)
        .await
        .unwrap()
        .data
        .unwrap();

    let component_ids: Vec<String> = data.components.edges.iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(component_ids.contains(&"demo1".to_string()));
    assert!(component_ids.contains(&"demo2".to_string()));
    assert!(component_ids.contains(&"blackhole1".to_string()));
    assert!(component_ids.contains(&"blackhole2".to_string()));
    assert_eq!(component_ids.len(), 4);

    // Verify new components are processing events
    wait_for_component_events(&client, "demo2", 10, EVENT_PROCESSING_TIMEOUT)
        .await
        .expect("New source should process events");

    // RELOAD 2: Remove components
    let config_with_removals = build_config_with_api(
        api_port,
        indoc! {"
            sources:
              demo1:
                type: demo_logs
                format: json
                interval: 0.1

            sinks:
              blackhole1:
                type: blackhole
                inputs: ['demo1']
        "},
    );

    overwrite_config_file(&config_path, &config_with_removals);
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGHUP).unwrap();
    sleep(RELOAD_TIME).await;

    // Verify removals
    let data: components_query::ResponseData = client
        .components_query(100)
        .await
        .unwrap()
        .data
        .unwrap();

    let component_ids: Vec<String> = data.components.edges.iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(component_ids.contains(&"demo1".to_string()));
    assert!(!component_ids.contains(&"demo2".to_string()));
    assert!(component_ids.contains(&"blackhole1".to_string()));
    assert!(!component_ids.contains(&"blackhole2".to_string()));
    assert_eq!(component_ids.len(), 2);

    cleanup_vector(vector);
}

#[tokio::test]
async fn watch_mode_auto_reloads() {
    let api_port = find_available_port();

    let initial_config = build_config_with_api(
        api_port,
        indoc! {"
            sources:
              demo:
                type: demo_logs
                format: json
                interval: 0.1

            sinks:
              blackhole:
                type: blackhole
                inputs: ['demo']
        "},
    );

    let config_path = create_config_file(&initial_config);

    // Start with watch flag
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c")
       .arg(&config_path)
       .arg("-w")  // Watch mode - auto reload on file change
       .env("VECTOR_DATA_DIR", create_data_directory());

    let vector = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start vector");

    sleep(STARTUP_TIME).await;

    let client = create_client(api_port);

    wait_for_api_ready(&client, API_READY_TIMEOUT)
        .await
        .expect("API never became ready");

    // Initial state: 1 source + 1 sink
    let data: components_query::ResponseData = client
        .components_query(100)
        .await
        .unwrap()
        .data
        .unwrap();
    let initial_count = data.components.edges.len();
    assert_eq!(initial_count, 2, "Should have source + sink");

    // Modify config - Vector should auto-reload
    let new_config = build_config_with_api(
        api_port,
        indoc! {"
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
        "},
    );

    overwrite_config_file(&config_path, &new_config);

    // Wait for auto-reload (no SIGHUP needed!)
    sleep(RELOAD_TIME).await;

    // Verify new component appeared
    let data: components_query::ResponseData = client
        .components_query(100)
        .await
        .unwrap()
        .data
        .unwrap();

    let component_ids: Vec<String> = data.components.edges.iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(component_ids.contains(&"demo2".to_string()));
    assert_eq!(component_ids.len(), 3);

    cleanup_vector(vector);
}
