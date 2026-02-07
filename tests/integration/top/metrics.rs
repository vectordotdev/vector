//! Tests for component discovery and metrics collection
//!
//! Verifies that `vector top` can discover components and observe
//! event counts flowing through the pipeline.

use indoc::indoc;

use super::harness::TestHarness;

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

    // Query metrics for all components
    let data = runner
        .query_components()
        .await
        .expect("Failed to query components");

    // Verify all components are discovered
    let component_ids: Vec<String> = data
        .components
        .edges
        .iter()
        .map(|e| e.node.component_id.clone())
        .collect();

    assert!(component_ids.contains(&"demo".to_string()));
    assert!(component_ids.contains(&"blackhole1".to_string()));
    assert!(component_ids.contains(&"blackhole2".to_string()));
    assert_eq!(component_ids.len(), 3);

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
