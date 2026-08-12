//! Integration tests for `vector tap` command
//!
//! Provides extensions for gRPC streaming and tests for event streaming.

use super::{common::*, harness::*};
use indoc::indoc;
use std::time::{Duration, Instant};
use tokio_stream::StreamExt;
use vector_lib::api_client::proto::{StreamOutputEventsRequest, StreamOutputEventsResponse};

pub const TAP_TIMEOUT: Duration = Duration::from_secs(10);

impl TestHarness {
    /// Collects tap events from the given patterns
    ///
    /// This is a simplified version that collects all events inline without storing a stream.
    pub async fn tap_and_collect(
        &mut self,
        outputs_patterns: &[&str],
        count: usize,
    ) -> Result<Vec<StreamOutputEventsResponse>, String> {
        const DEFAULT_LIMIT: i32 = 1000;
        const DEFAULT_INTERVAL_MS: i32 = 100;

        let request = StreamOutputEventsRequest {
            outputs_patterns: outputs_patterns.iter().map(|s| s.to_string()).collect(),
            inputs_patterns: vec![],
            limit: DEFAULT_LIMIT,
            interval_ms: DEFAULT_INTERVAL_MS,
        };

        let mut stream = self
            .api_client()
            .stream_output_events(request)
            .await
            .map_err(|e| format!("Failed to create tap stream: {e}"))?;

        let start = Instant::now();
        let mut events = Vec::new();

        while events.len() < count {
            if start.elapsed() >= TAP_TIMEOUT {
                return Err(format!(
                    "Timeout: collected {}/{} events in {:?}",
                    events.len(),
                    count,
                    TAP_TIMEOUT
                ));
            }

            match tokio::time::timeout(TAP_TIMEOUT - start.elapsed(), stream.next()).await {
                Ok(Some(Ok(event))) => {
                    events.push(event);
                }
                Ok(Some(Err(e))) => {
                    return Err(format!("Stream error: {}", e));
                }
                Ok(None) => {
                    return Err(format!(
                        "Stream ended unexpectedly after {} events",
                        events.len()
                    ));
                }
                Err(_) => {
                    return Err("Timeout waiting for next event".to_string());
                }
            }
        }

        Ok(events)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn tap_receives_events() {
    let config = single_source_config("demo", 0.01, Some(100));
    let mut harness = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Tap the source output with wildcard pattern and collect events
    let events = harness
        .tap_and_collect(&["*"], 10)
        .await
        .expect("Should receive events");

    assert!(!events.is_empty(), "Should receive at least one event");

    // Verify we got at least one tapped event (not just notifications)
    use vector_lib::api_client::proto::stream_output_events_response::Event;
    let tapped_events: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.event {
            Some(Event::TappedEvent(tapped)) => Some(tapped),
            _ => None,
        })
        .collect();

    let notification_count = events
        .iter()
        .filter(|e| matches!(&e.event, Some(Event::Notification(_))))
        .count();

    assert!(
        !tapped_events.is_empty(),
        "Should receive at least one tapped event, got {} events total ({} notifications)",
        events.len(),
        notification_count
    );
}

#[tokio::test]
async fn tap_specific_component() {
    let config = dual_source_config("demo1", "demo2", 0.01, Some(100));
    let mut harness = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Tap only demo1, not demo2
    let events = harness
        .tap_and_collect(&["demo1"], 10)
        .await
        .expect("Should receive events");

    // Verify we only got events from demo1
    use vector_lib::api_client::proto::stream_output_events_response::Event;
    let tapped_events: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.event {
            Some(Event::TappedEvent(tapped)) => Some(tapped),
            _ => None,
        })
        .collect();

    assert!(!tapped_events.is_empty(), "Should receive tapped events");

    // All tapped events should be from demo1
    for tapped in &tapped_events {
        assert_eq!(
            tapped.component_id, "demo1",
            "Should only receive events from demo1"
        );
    }
}

#[tokio::test]
async fn tap_survives_config_reload() {
    use vector_lib::api_client::proto::stream_output_events_response::Event;

    let config = single_source_config("demo", 0.05, None); // No count limit - runs continuously
    let mut harness = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Open the stream ONCE and keep it alive across the reload.
    // tonic channels are Arc-based so the returned stream is owned and does not
    // borrow `harness`, allowing us to call reload_with_config below.
    let request = StreamOutputEventsRequest {
        outputs_patterns: vec!["*".to_string()],
        inputs_patterns: vec![],
        limit: 1000,
        interval_ms: 100,
    };
    let mut stream = harness
        .api_client()
        .stream_output_events(request)
        .await
        .expect("Failed to open tap stream");

    // Collect a few pre-reload events to confirm the stream is working
    let mut pre_reload_count = 0;
    let start = Instant::now();
    while pre_reload_count < 5 {
        assert!(
            start.elapsed() < TAP_TIMEOUT,
            "Timeout waiting for pre-reload events"
        );
        match tokio::time::timeout(TAP_TIMEOUT - start.elapsed(), stream.next()).await {
            Ok(Some(Ok(_))) => pre_reload_count += 1,
            Ok(Some(Err(e))) => panic!("Stream error before reload: {e}"),
            Ok(None) => panic!("Stream ended unexpectedly before reload"),
            Err(_) => panic!("Timeout before reload"),
        }
    }

    // Reload with completely new components
    harness
        .reload_with_config(
            indoc! {"
                sources:
                  tap_demo1:
                    type: demo_logs
                    format: json
                    interval: 0.05

                  tap_demo2:
                    type: demo_logs
                    format: json
                    interval: 0.05

                sinks:
                  tap_blackhole:
                    type: blackhole
                    inputs: ['tap_demo1', 'tap_demo2']
            "},
            &["tap_demo1", "tap_demo2", "tap_blackhole"],
        )
        .await
        .expect("Failed to reload config");

    // Continue reading from the SAME stream. If the tap implementation drops active
    // streams on reload, stream.next() will return None and the test will fail.
    let mut post_reload_count = 0;
    let start = Instant::now();
    while post_reload_count < 5 {
        assert!(
            start.elapsed() < TAP_TIMEOUT,
            "Stream did not survive reload: only received {post_reload_count} tapped events after reload"
        );
        match tokio::time::timeout(TAP_TIMEOUT - start.elapsed(), stream.next()).await {
            Ok(Some(Ok(event))) => {
                if matches!(&event.event, Some(Event::TappedEvent(_))) {
                    post_reload_count += 1;
                }
                // Notifications (component added/removed) are expected during reload - skip them
            }
            Ok(Some(Err(e))) => panic!("Stream error after reload: {e}"),
            Ok(None) => panic!("Stream was closed on reload - tap subscription was dropped"),
            Err(_) => panic!("Timeout waiting for post-reload tapped events"),
        }
    }
}

#[tokio::test]
async fn multiple_concurrent_subscriptions() {
    let config = dual_source_config("demo1", "demo2", 0.01, Some(100));
    let mut harness = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Create two separate tap requests using the same harness
    let events1 = harness
        .tap_and_collect(&["demo1"], 5)
        .await
        .expect("Should receive events from tap1");

    let events2 = harness
        .tap_and_collect(&["demo2"], 5)
        .await
        .expect("Should receive events from tap2");

    assert!(!events1.is_empty(), "Tap1 should receive events");
    assert!(!events2.is_empty(), "Tap2 should receive events");

    // Verify we got at least the requested number of events (may get more due to batching)
    assert!(
        events1.len() >= 5,
        "Should receive at least 5 events from tap1, got {}",
        events1.len()
    );
    assert!(
        events2.len() >= 5,
        "Should receive at least 5 events from tap2, got {}",
        events2.len()
    );

    // Verify tap1 only sees demo1
    use vector_lib::api_client::proto::stream_output_events_response::Event;
    let tapped_events1: Vec<_> = events1
        .iter()
        .filter_map(|e| match &e.event {
            Some(Event::TappedEvent(tapped)) => Some(tapped),
            _ => None,
        })
        .collect();

    for tapped in &tapped_events1 {
        assert_eq!(
            tapped.component_id, "demo1",
            "Tap1 should only see demo1 events"
        );
    }

    // Verify tap2 only sees demo2
    let tapped_events2: Vec<_> = events2
        .iter()
        .filter_map(|e| match &e.event {
            Some(Event::TappedEvent(tapped)) => Some(tapped),
            _ => None,
        })
        .collect();

    for tapped in &tapped_events2 {
        assert_eq!(
            tapped.component_id, "demo2",
            "Tap2 should only see demo2 events"
        );
    }

    // Verify we got different sets of events
    assert_ne!(
        tapped_events1.len(),
        0,
        "Should have some tapped events from demo1"
    );
    assert_ne!(
        tapped_events2.len(),
        0,
        "Should have some tapped events from demo2"
    );

    // Create new taps to the same components to verify repeatability
    let events1_again = harness
        .tap_and_collect(&["demo1"], 5)
        .await
        .expect("Should receive events from tap1 again");

    let events2_again = harness
        .tap_and_collect(&["demo2"], 5)
        .await
        .expect("Should receive events from tap2 again");

    // Verify second round of taps still work
    assert!(
        !events1_again.is_empty(),
        "Tap1 again should receive events"
    );
    assert!(
        !events2_again.is_empty(),
        "Tap2 again should receive events"
    );

    // Verify component isolation is maintained
    let tapped_events1_again: Vec<_> = events1_again
        .iter()
        .filter_map(|e| match &e.event {
            Some(Event::TappedEvent(tapped)) => Some(tapped),
            _ => None,
        })
        .collect();

    let tapped_events2_again: Vec<_> = events2_again
        .iter()
        .filter_map(|e| match &e.event {
            Some(Event::TappedEvent(tapped)) => Some(tapped),
            _ => None,
        })
        .collect();

    assert!(
        !tapped_events1_again.is_empty(),
        "Should have tapped events from demo1 again"
    );
    assert!(
        !tapped_events2_again.is_empty(),
        "Should have tapped events from demo2 again"
    );

    for tapped in &tapped_events1_again {
        assert_eq!(
            tapped.component_id, "demo1",
            "Second tap1 should only see demo1 events"
        );
    }

    for tapped in &tapped_events2_again {
        assert_eq!(
            tapped.component_id, "demo2",
            "Second tap2 should only see demo2 events"
        );
    }
}
