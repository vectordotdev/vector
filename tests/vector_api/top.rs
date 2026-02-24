//! Integration tests for `vector top` command
//!
//! Provides extensions for GraphQL queries and tests for component discovery,
//! metrics collection, and config reloading.

use super::{common::*, harness::*};
use indoc::indoc;

impl TestHarness {
    /// Queries all components from the GraphQL API
    pub async fn query_components(
        &self,
    ) -> Result<vector_lib::api_client::gql::components_query::ResponseData, String> {
        use vector_lib::api_client::gql::ComponentsQueryExt;

        self.api_client()
            .components_query(100)
            .await
            .map_err(|e| format!("Query failed: {e}"))?
            .data
            .ok_or_else(|| "No data in response".to_string())
    }

    /// Queries component IDs from the GraphQL API
    pub async fn query_component_ids(&self) -> Result<Vec<String>, String> {
        let data = self.query_components().await?;
        Ok(data
            .components
            .edges
            .iter()
            .map(|e| e.node.component_id.clone())
            .collect())
    }

    /// Waits for a component to process at least the expected number of events
    pub async fn wait_for_events(
        &self,
        component_id: &str,
        expected_events: i64,
    ) -> Result<i64, String> {
        wait_for_component_events(
            self.api_client(),
            component_id,
            expected_events,
            EVENT_PROCESSING_TIMEOUT,
        )
        .await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn displays_pipeline_topology_and_metrics() {
    const EXPECTED_EVENTS: i64 = 100;

    let runner = TestHarness::new(indoc! {"
        sources:
          demo:
            type: demo_logs
            format: json
            interval: 0.01

        sinks:
          blackhole1:
            type: blackhole
            inputs: ['demo']

          blackhole2:
            type: blackhole
            inputs: ['demo']
    "})
    .await
    .expect("Failed to start Vector");

    // Wait for events to flow
    runner
        .wait_for_events("demo", EXPECTED_EVENTS)
        .await
        .expect("Source never sent expected events");

    // Verify all components are discovered
    let component_ids = runner
        .query_component_ids()
        .await
        .expect("Failed to query component IDs");

    assert!(component_ids.contains(&"demo".to_string()));
    assert!(component_ids.contains(&"blackhole1".to_string()));
    assert!(component_ids.contains(&"blackhole2".to_string()));
    assert_eq!(component_ids.len(), 3);

    // Query full component data for types and metrics
    let data = runner
        .query_components()
        .await
        .expect("Failed to query components");

    // Verify component types are reported correctly
    for edge in &data.components.edges {
        let component_id = &edge.node.component_id;
        let component_type = edge.node.on.to_string();

        match component_id.as_str() {
            "demo" => assert_eq!(component_type, "source"),
            "blackhole1" | "blackhole2" => assert_eq!(component_type, "sink"),
            _ => panic!("Unexpected component: {}", component_id),
        }
    }

    // Verify source sent events
    let source = data
        .components
        .edges
        .iter()
        .find(|e| e.node.component_id == "demo")
        .expect("Should find source");

    assert!(source.node.on.sent_events_total() >= EXPECTED_EVENTS);

    // Verify at least one sink received events
    let sink1 = data
        .components
        .edges
        .iter()
        .find(|e| e.node.component_id == "blackhole1")
        .expect("Should find sink1");

    assert!(sink1.node.on.received_events_total() >= EXPECTED_EVENTS);
}

#[tokio::test]
async fn config_reload_updates_components() {
    // Initial config: 1 source -> 1 sink
    let config = single_source_config("demo1", 0.1, None);
    let mut runner = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Verify initial components
    let component_ids = runner.query_component_ids().await.expect("Failed to query");

    assert!(component_ids.contains(&"demo1".to_string()));
    assert!(component_ids.contains(&"blackhole".to_string()));
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

    let component_ids = runner.query_component_ids().await.expect("Failed to query");

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
    let component_ids = runner.query_component_ids().await.expect("Failed to query");

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
    let config = single_source_config("demo", 0.1, None);
    let mut runner = TestHarness::new_with_watch_mode(&config)
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
