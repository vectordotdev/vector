//! Tests for config reloading and metric updates
//!
//! Verifies that `vector top` reflects changes after Vector reloads
//! its configuration.

use indoc::indoc;

use super::harness::TestHarness;

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
        .reload_with_config(
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
            &["demo1", "demo2", "blackhole1", "blackhole2"],
        )
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

    // RELOAD 2: Replace with completely new components (same count, different names)
    runner
        .reload_with_config(
            indoc! {"
                sources:
                  new_demo:
                    type: demo_logs
                    format: json
                    interval: 0.1

                sinks:
                  new_blackhole:
                    type: blackhole
                    inputs: ['new_demo']
            "},
            &["new_demo", "new_blackhole"],
        )
        .await
        .expect("Failed to reload config");

    // Verify old components removed and new ones added
    let data = runner.query_components().await.expect("Failed to query");

    let component_ids: Vec<String> = data
        .components
        .edges
        .iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(!component_ids.contains(&"demo1".to_string()));
    assert!(!component_ids.contains(&"demo2".to_string()));
    assert!(!component_ids.contains(&"blackhole1".to_string()));
    assert!(!component_ids.contains(&"blackhole2".to_string()));
    assert!(component_ids.contains(&"new_demo".to_string()));
    assert!(component_ids.contains(&"new_blackhole".to_string()));
    assert_eq!(component_ids.len(), 2);
}

#[tokio::test]
async fn watch_mode_auto_reloads() {
    // Start Vector with watch mode enabled
    let mut runner = TestHarness::new_with_watch_mode(indoc! {"
        sources:
          demo:
            type: demo_logs
            format: json
            interval: 0.1

        sinks:
          blackhole:
            type: blackhole
            inputs: ['demo']
    "})
    .await
    .expect("Failed to start Vector");

    // Verify initial state: 1 source + 1 sink
    let data = runner.query_components().await.expect("Failed to query");
    assert_eq!(data.components.edges.len(), 2);

    // Modify config file - Vector should auto-reload (no SIGHUP needed!)
    runner
        .reload_with_config(
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
            &["demo", "demo2", "blackhole"],
        )
        .await
        .expect("Watch mode reload failed");

    // Cleanup happens automatically via Drop
}
