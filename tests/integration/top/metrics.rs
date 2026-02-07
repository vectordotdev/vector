//! Tests for component discovery and metrics collection
//!
//! Verifies that `vector top` can discover components and observe
//! event counts flowing through the pipeline.

use indoc::indoc;
use super::*;
use tokio::time::sleep;
use vector_lib::api_client::gql::ComponentsQueryExt;

#[tokio::test]
async fn displays_pipeline_topology_and_metrics() {
    const EXPECTED_EVENTS: i64 = 100;
    let api_port = find_available_port();

    let config = build_config_with_api(
        api_port,
        indoc! {"
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
        "},
    );

    let mut vector = spawn_vector_with_api(&config);

    sleep(STARTUP_TIME).await;

    assert_eq!(None, vector.try_wait().unwrap(), "Vector exited too early");

    let client = create_client(api_port);

    wait_for_api_ready(&client, API_READY_TIMEOUT)
        .await
        .expect("API never became ready");

    // Wait for events to flow
    wait_for_component_events(&client, "demo", EXPECTED_EVENTS, EVENT_PROCESSING_TIMEOUT)
        .await
        .expect("Source never sent expected events");

    // Query metrics for all components
    let data: components_query::ResponseData = client
        .components_query(100)
        .await
        .unwrap()
        .data
        .unwrap();

    // Verify all components are discovered
    let component_ids: Vec<String> = data.components.edges.iter()
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
    let source = data.components.edges.iter()
        .find(|e| e.node.component_id == "demo")
        .expect("Should find source");

    assert!(source.node.on.sent_events_total() >= EXPECTED_EVENTS);

    // Verify at least one sink received events
    let sink1 = data.components.edges.iter()
        .find(|e| e.node.component_id == "blackhole1")
        .expect("Should find sink1");

    assert!(sink1.node.on.received_events_total() >= EXPECTED_EVENTS);

    cleanup_vector(vector);
}
