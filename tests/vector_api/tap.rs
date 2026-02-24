//! Integration tests for `vector tap` command
//!
//! Provides extensions for WebSocket subscriptions and tests for event streaming.

use super::{common::*, harness::*};
use indoc::indoc;
use std::time::{Duration, Instant};
use tokio_stream::StreamExt;
use vector_lib::api_client::{
    connect_subscription_client,
    gql::{
        TapEncodingFormat, TapSubscriptionExt,
        output_events_by_component_id_patterns_subscription::OutputEventsByComponentIdPatternsSubscriptionOutputEventsByComponentIdPatterns as TapEvent,
    },
};

pub const TAP_TIMEOUT: Duration = Duration::from_secs(10);

impl TestHarness {
    /// Returns WebSocket URL for subscriptions
    fn websocket_url(&self) -> url::Url {
        format!("ws://127.0.0.1:{}/graphql", self.api_port())
            .parse()
            .expect("Valid WebSocket URL")
    }

    /// Creates a tap subscription for the given patterns
    ///
    /// Uses sensible defaults: format=JSON, limit=1000, interval=100ms
    pub async fn tap_subscription(
        &self,
        outputs_patterns: &[&str],
        inputs_patterns: &[&str],
    ) -> Result<TapSubscription, String> {
        const DEFAULT_LIMIT: i64 = 1000;
        const DEFAULT_INTERVAL_MS: i64 = 100;
        let url = self.websocket_url();

        let subscription_client = connect_subscription_client(url)
            .await
            .map_err(|e| format!("Failed to connect to WebSocket: {e}"))?;

        let stream = subscription_client.output_events_by_component_id_patterns_subscription(
            outputs_patterns.iter().map(|s| s.to_string()).collect(),
            inputs_patterns.iter().map(|s| s.to_string()).collect(),
            TapEncodingFormat::Json,
            DEFAULT_LIMIT,
            DEFAULT_INTERVAL_MS,
        );

        Ok(TapSubscription {
            stream,
            _client: subscription_client,
        })
    }

    /// Creates a tap subscription and collects initial events
    ///
    /// Returns both the collected events and the subscription handle.
    /// If you need to collect more events later, keep the handle; otherwise it will be dropped.
    /// For input pattern filtering, use `tap_subscription` directly.
    pub async fn tap_and_collect(
        &self,
        outputs_patterns: &[&str],
        count: usize,
    ) -> Result<(Vec<TapEvent>, TapSubscription), String> {
        let mut tap = self.tap_subscription(outputs_patterns, &[]).await?;
        let events = tap.take_events(count, TAP_TIMEOUT).await?;
        Ok((events, tap))
    }
}

/// Wrapper around a tap subscription stream with helper methods
pub struct TapSubscription {
    stream: vector_lib::api_client::BoxedSubscription<
        vector_lib::api_client::gql::OutputEventsByComponentIdPatternsSubscription,
    >,
    _client: vector_lib::api_client::SubscriptionClient, // Keep client alive!
}

impl TapSubscription {
    /// Collects tap events until count is reached or timeout occurs
    pub async fn take_events(
        &mut self,
        count: usize,
        timeout: Duration,
    ) -> Result<Vec<TapEvent>, String> {
        let start = Instant::now();
        let mut events = Vec::new();

        while events.len() < count {
            if start.elapsed() >= timeout {
                return Err(format!(
                    "Timeout: collected {}/{} events in {:?}",
                    events.len(),
                    count,
                    timeout
                ));
            }

            match tokio::time::timeout(timeout - start.elapsed(), self.stream.as_mut().next()).await
            {
                Ok(Some(Some(response))) => {
                    if let Some(data) = response.data {
                        for event in data.output_events_by_component_id_patterns {
                            events.push(event);
                        }
                    } else if let Some(errors) = response.errors
                        && !errors.is_empty()
                    {
                        return Err(format!("GraphQL errors: {:?}", errors));
                    }
                }
                Ok(Some(None)) => {
                    // No response in this poll, continue
                    continue;
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
    let harness = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Tap the source output with wildcard pattern and collect events
    let (events, _tap) = harness
        .tap_and_collect(&["*"], 10)
        .await
        .expect("Should receive events");

    assert!(!events.is_empty(), "Should receive at least one event");

    // Verify we got at least one log event (not just notifications)
    let log_events: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let TapEvent::Log(log) = e {
                Some(log)
            } else {
                None
            }
        })
        .collect();

    assert!(
        !log_events.is_empty(),
        "Should receive at least one log event, got {} events total ({} notifications)",
        events.len(),
        events
            .iter()
            .filter(|e| matches!(e, TapEvent::EventNotification(_)))
            .count()
    );
}

#[tokio::test]
async fn tap_specific_component() {
    let config = dual_source_config("demo1", "demo2", 0.01, Some(100));
    let harness = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Tap only demo1, not demo2
    let (events, _tap) = harness
        .tap_and_collect(&["demo1"], 10)
        .await
        .expect("Should receive events");

    // Verify we only got events from demo1
    let log_events: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let TapEvent::Log(log) = e {
                Some(log)
            } else {
                None
            }
        })
        .collect();

    assert!(!log_events.is_empty(), "Should receive log events");

    // All log events should be from demo1
    for log in &log_events {
        assert_eq!(
            log.component_id, "demo1",
            "Should only receive events from demo1"
        );
    }
}

#[tokio::test]
async fn tap_survives_config_reload() {
    let config = single_source_config("demo", 0.05, Some(100));
    let mut harness = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Start tap with wildcard and collect initial events
    let (initial_events, mut tap) = harness
        .tap_and_collect(&["*"], 5)
        .await
        .expect("Should receive initial events");

    assert!(!initial_events.is_empty(), "Should receive initial events");

    // Reload config with a new component
    harness
        .reload_with_config(
            indoc! {"
                sources:
                  demo:
                    type: demo_logs
                    format: json
                    interval: 0.05
                    count: 100

                  new_demo:
                    type: demo_logs
                    format: json
                    interval: 0.05
                    count: 100

                sinks:
                  blackhole:
                    type: blackhole
                    inputs: ['demo', 'new_demo']
            "},
            &["demo", "new_demo", "blackhole"],
        )
        .await
        .expect("Failed to reload config");

    // Tap should still work and see events from both sources
    let after_reload = tap
        .take_events(5, TAP_TIMEOUT)
        .await
        .expect("Should receive events after reload");

    assert!(
        !after_reload.is_empty(),
        "Should receive events after reload"
    );

    let log_events: Vec<_> = after_reload
        .iter()
        .filter_map(|e| {
            if let TapEvent::Log(log) = e {
                Some(log)
            } else {
                None
            }
        })
        .collect();

    assert!(
        !log_events.is_empty(),
        "Should receive log events after reload"
    );
}

#[tokio::test]
async fn multiple_concurrent_subscriptions() {
    let config = dual_source_config("demo1", "demo2", 0.01, Some(100));
    let harness = TestHarness::new(&config)
        .await
        .expect("Failed to start Vector");

    // Create two separate tap subscriptions using the same harness
    let (events1, _tap1) = harness
        .tap_and_collect(&["demo1"], 5)
        .await
        .expect("Should receive events from tap1");

    let (events2, _tap2) = harness
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
    let log_events1: Vec<_> = events1
        .iter()
        .filter_map(|e| {
            if let TapEvent::Log(log) = e {
                Some(log)
            } else {
                None
            }
        })
        .collect();

    for log in &log_events1 {
        assert_eq!(
            log.component_id, "demo1",
            "Tap1 should only see demo1 events"
        );
    }

    // Verify tap2 only sees demo2
    let log_events2: Vec<_> = events2
        .iter()
        .filter_map(|e| {
            if let TapEvent::Log(log) = e {
                Some(log)
            } else {
                None
            }
        })
        .collect();

    for log in &log_events2 {
        assert_eq!(
            log.component_id, "demo2",
            "Tap2 should only see demo2 events"
        );
    }

    // Verify we got different sets of events
    assert_ne!(
        log_events1.len(),
        0,
        "Should have some log events from demo1"
    );
    assert_ne!(
        log_events2.len(),
        0,
        "Should have some log events from demo2"
    );

    // Create new taps to the same components to verify repeatability
    let (events1_again, _tap1_again) = harness
        .tap_and_collect(&["demo1"], 5)
        .await
        .expect("Should receive events from tap1 again");

    let (events2_again, _tap2_again) = harness
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
    let log_events1_again: Vec<_> = events1_again
        .iter()
        .filter_map(|e| {
            if let TapEvent::Log(log) = e {
                Some(log)
            } else {
                None
            }
        })
        .collect();

    let log_events2_again: Vec<_> = events2_again
        .iter()
        .filter_map(|e| {
            if let TapEvent::Log(log) = e {
                Some(log)
            } else {
                None
            }
        })
        .collect();

    assert!(
        !log_events1_again.is_empty(),
        "Should have log events from demo1 again"
    );
    assert!(
        !log_events2_again.is_empty(),
        "Should have log events from demo2 again"
    );

    for log in &log_events1_again {
        assert_eq!(
            log.component_id, "demo1",
            "Second tap1 should only see demo1 events"
        );
    }

    for log in &log_events2_again {
        assert_eq!(
            log.component_id, "demo2",
            "Second tap2 should only see demo2 events"
        );
    }
}
