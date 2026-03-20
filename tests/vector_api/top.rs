//! Integration tests for `vector top` command
//!
//! Provides extensions for gRPC queries and tests for component discovery,
//! metrics collection, and config reloading.

use super::{common::*, harness::*};
use indoc::indoc;
use tokio_stream::StreamExt;
use vector_lib::api_client::proto::{
    GetComponentsResponse, MetricName, stream_component_metrics_response::Value,
};

impl TestHarness {
    /// Queries all components from the gRPC API
    pub async fn query_components(&mut self) -> Result<GetComponentsResponse, String> {
        self.api_client()
            .get_components(100)
            .await
            .map_err(|e| format!("Query failed: {e}"))
    }

    /// Queries component IDs from the gRPC API
    pub async fn query_component_ids(&mut self) -> Result<Vec<String>, String> {
        let response = self.query_components().await?;
        Ok(response
            .components
            .iter()
            .map(|c| c.component_id.clone())
            .collect())
    }

    /// Waits for a component to process at least the expected number of events
    pub async fn wait_for_events(
        &mut self,
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

    let mut runner = TestHarness::new(indoc! {"
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
    let response = runner
        .query_components()
        .await
        .expect("Failed to query components");

    // Verify component types are reported correctly
    use vector_lib::api_client::proto::ComponentType;
    for component in &response.components {
        let component_id = &component.component_id;
        let component_type =
            ComponentType::try_from(component.component_type).expect("Valid component type");

        match component_id.as_str() {
            "demo" => assert_eq!(component_type, ComponentType::Source),
            "blackhole1" | "blackhole2" => assert_eq!(component_type, ComponentType::Sink),
            _ => panic!("Unexpected component: {}", component_id),
        }
    }

    // Verify source sent events
    let source = response
        .components
        .iter()
        .find(|c| c.component_id == "demo")
        .expect("Should find source");

    let sent_events = source
        .metrics
        .as_ref()
        .and_then(|m| m.sent_events_total)
        .unwrap_or(0);
    assert!(sent_events >= EXPECTED_EVENTS);

    // Verify at least one sink received events
    let sink1 = response
        .components
        .iter()
        .find(|c| c.component_id == "blackhole1")
        .expect("Should find sink1");

    let received_events = sink1
        .metrics
        .as_ref()
        .and_then(|m| m.received_events_total)
        .unwrap_or(0);
    assert!(received_events >= EXPECTED_EVENTS);
}

#[tokio::test]
async fn config_reload_updates_components() {
    // Initial config: 1 source -> 1 sink (runs continuously)
    let config = single_source_config("demo1", 0.1, None);
    let mut runner = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Verify initial components
    let component_ids = runner.query_component_ids().await.expect("Failed to query");

    assert!(component_ids.contains(&"demo1".to_string()));
    assert!(component_ids.contains(&"blackhole".to_string()));
    assert_eq!(component_ids.len(), 2);

    // RELOAD 1: Replace with completely new set
    runner
        .reload_with_config(
            indoc! {"
                sources:
                  new_demo1:
                    type: demo_logs
                    format: json
                    interval: 0.1

                  new_demo2:
                    type: demo_logs
                    format: json
                    interval: 0.1

                sinks:
                  new_blackhole1:
                    type: blackhole
                    inputs: ['new_demo1']

                  new_blackhole2:
                    type: blackhole
                    inputs: ['new_demo2']
            "},
            &["new_demo1", "new_demo2", "new_blackhole1", "new_blackhole2"],
        )
        .await
        .expect("Failed to reload config");

    // Verify old components removed and new ones added
    assert!(runner.check_running(), "Vector should still be running");

    let component_ids = runner.query_component_ids().await.expect("Failed to query");

    assert!(!component_ids.contains(&"demo1".to_string()));
    assert!(!component_ids.contains(&"blackhole".to_string()));
    assert!(component_ids.contains(&"new_demo1".to_string()));
    assert!(component_ids.contains(&"new_demo2".to_string()));
    assert!(component_ids.contains(&"new_blackhole1".to_string()));
    assert!(component_ids.contains(&"new_blackhole2".to_string()));
    assert_eq!(component_ids.len(), 4);

    // Verify new components are processing events
    runner
        .wait_for_events("new_demo1", 10)
        .await
        .expect("New source should process events");

    // RELOAD 2: Scale back down to single component
    runner
        .reload_with_config(
            indoc! {"
                sources:
                  final_demo:
                    type: demo_logs
                    format: json
                    interval: 0.1

                sinks:
                  final_blackhole:
                    type: blackhole
                    inputs: ['final_demo']
            "},
            &["final_demo", "final_blackhole"],
        )
        .await
        .expect("Failed to reload config");

    // Verify all previous components removed and final ones added
    let component_ids = runner.query_component_ids().await.expect("Failed to query");

    assert!(!component_ids.contains(&"new_demo1".to_string()));
    assert!(!component_ids.contains(&"new_demo2".to_string()));
    assert!(!component_ids.contains(&"new_blackhole1".to_string()));
    assert!(!component_ids.contains(&"new_blackhole2".to_string()));
    assert!(component_ids.contains(&"final_demo".to_string()));
    assert!(component_ids.contains(&"final_blackhole".to_string()));
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
    let response = runner.query_components().await.expect("Failed to query");
    assert_eq!(response.components.len(), 2);

    // Modify config file - Vector should auto-reload (no SIGHUP needed!)
    // Use completely new component names to avoid reload issues
    runner
        .reload_with_config(
            indoc! {"
                sources:
                  watch_demo1:
                    type: demo_logs
                    format: json
                    interval: 0.1

                  watch_demo2:
                    type: demo_logs
                    format: json
                    interval: 0.1

                sinks:
                  watch_blackhole:
                    type: blackhole
                    inputs: ['watch_demo1', 'watch_demo2']
            "},
            &["watch_demo1", "watch_demo2", "watch_blackhole"],
        )
        .await
        .expect("Watch mode reload failed");

    // Verify reload worked
    let component_ids = runner.query_component_ids().await.expect("Failed to query");
    assert_eq!(component_ids.len(), 3);

    // Cleanup happens automatically via Drop
}

#[tokio::test]
async fn multi_output_transform_reports_per_output_sent_events() {
    // Pipeline: demo_logs -> route (two named outputs) -> two blackhole sinks
    // The route transform splits events into "all" (everything) and "has_host" (subset).
    // We verify that:
    //   1. GetComponents reports two Output entries for the route transform
    //   2. StreamComponentSentEventsTotal populates output_totals for the route transform
    const EXPECTED_EVENTS: i64 = 50;

    let mut runner = TestHarness::new(indoc! {"
        sources:
          demo:
            type: demo_logs
            format: json
            interval: 0.01

        transforms:
          splitter:
            type: route
            inputs: ['demo']
            route:
              all: 'true'
              has_host: 'exists(.host)'

        sinks:
          sink_all:
            type: blackhole
            inputs: ['splitter.all']

          sink_has_host:
            type: blackhole
            inputs: ['splitter.has_host']
    "})
    .await
    .expect("Failed to start Vector");

    // Wait for events to reach the sinks
    runner
        .wait_for_events("demo", EXPECTED_EVENTS)
        .await
        .expect("Source never sent expected events");

    // --- Assert 1: GetComponents reports per-output entries for the route transform ---
    let response = runner
        .query_components()
        .await
        .expect("Failed to query components");

    let splitter = response
        .components
        .iter()
        .find(|c| c.component_id == "splitter")
        .expect("splitter transform not found");

    // Route transform must report at least 2 named outputs
    assert!(
        splitter.outputs.len() >= 2,
        "Expected >= 2 outputs for route transform, got {}",
        splitter.outputs.len()
    );

    let output_ids: Vec<&str> = splitter
        .outputs
        .iter()
        .map(|o| o.output_id.as_str())
        .collect();
    assert!(
        output_ids.contains(&"all"),
        "Missing 'all' output, got: {:?}",
        output_ids
    );
    assert!(
        output_ids.contains(&"has_host"),
        "Missing 'has_host' output, got: {:?}",
        output_ids
    );

    // --- Assert 2: StreamComponentMetrics(SentEventsTotal) populates output_totals ---
    let mut stream = runner
        .api_client()
        .stream_component_metrics(MetricName::SentEventsTotal, 500)
        .await
        .expect("Failed to open sent_events_total stream");

    // Drain stream updates until we get output_totals for the splitter, or timeout
    let deadline = tokio::time::Instant::now() + EVENT_PROCESSING_TIMEOUT;
    let mut splitter_totals_found = false;

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(600), stream.next()).await {
            Ok(Some(Ok(msg))) => {
                if msg.component_id == "splitter"
                    && let Some(Value::Total(total)) = msg.value
                    && !total.output_totals.is_empty()
                {
                    // Both named outputs must be present with positive counts
                    let all_total = total.output_totals.get("all").copied().unwrap_or(0);
                    let has_host_total = total.output_totals.get("has_host").copied().unwrap_or(0);

                    assert!(
                        all_total > 0,
                        "Expected positive total for 'all' output, got {}",
                        all_total
                    );
                    assert!(
                        has_host_total > 0,
                        "Expected positive total for 'has_host' output, got {}",
                        has_host_total
                    );
                    // "all" must be >= "has_host" since it matches every event
                    assert!(
                        all_total >= has_host_total,
                        "'all' ({}) should be >= 'has_host' ({})",
                        all_total,
                        has_host_total
                    );

                    splitter_totals_found = true;
                    break;
                }
            }
            Ok(Some(Err(e))) => panic!("Stream error: {e}"),
            Ok(None) => panic!("Stream ended unexpectedly"),
            Err(_) => continue, // timeout on this tick, keep looping
        }
    }

    assert!(
        splitter_totals_found,
        "Never received non-empty output_totals for splitter transform within timeout"
    );
}
