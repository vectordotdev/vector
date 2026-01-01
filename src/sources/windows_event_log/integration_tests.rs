#![cfg(feature = "sources-windows_event_log-integration-tests")]
#![cfg(test)]

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::time::timeout;
use vector_lib::config::LogNamespace;

use super::*;
use crate::{
    config::{SourceConfig, SourceContext},
    test_util::{
        components::{SourceTestRunner, run_and_assert_source_compliance},
        random_string,
    },
};

/// Test Windows Event Log source against real Windows Event Log service
#[tokio::test]
async fn test_windows_eventlog_integration() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string(), "Application".to_string()],
        connection_timeout_secs: 30,
        event_timeout_ms: 5000,
        batch_size: 10,
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(5), &[]).await;
    assert!(
        !events.is_empty(),
        "Should receive at least one event from Windows Event Log"
    );

    // Verify event structure
    let event = &events[0];
    assert!(event.as_log().contains("timestamp"));
    assert!(event.as_log().contains("message"));
    assert!(event.as_log().contains("source"));
    assert!(event.as_log().contains("channel"));
}

/// Test Windows Event Log source with specific channel filtering
#[tokio::test]
async fn test_windows_eventlog_channel_filtering() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        connection_timeout_secs: 30,
        event_timeout_ms: 3000,
        batch_size: 5,
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(3), &[]).await;

    // All events should be from System channel
    for event in events.iter().take(5) {
        if let Some(channel) = event.as_log().get("channel") {
            assert_eq!(channel.to_string_lossy(), "System");
        }
    }
}

/// Test Windows Event Log source with event-driven subscription (no polling)
#[tokio::test]
async fn test_windows_eventlog_event_driven() {
    let config = WindowsEventLogConfig {
        channels: vec!["Application".to_string()],
        connection_timeout_secs: 10,
        event_timeout_ms: 2000,
        batch_size: 5,
        read_existing_events: false, // Only new events
        ..Default::default()
    };

    // Test shorter duration since we're not polling
    let events = run_and_assert_source_compliance(config, Duration::from_secs(2), &[]).await;

    // Verify events are properly structured
    for event in events.iter().take(3) {
        let log = event.as_log();
        assert!(log.contains("timestamp"), "Event should have timestamp");
        assert!(log.contains("event_id"), "Event should have event_id");
        assert!(log.contains("level"), "Event should have level");
    }
}

/// Test Windows Event Log source with XML inclusion
#[tokio::test]
async fn test_windows_eventlog_with_xml() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        connection_timeout_secs: 30,
        event_timeout_ms: 5000,
        batch_size: 3,
        include_xml: true,
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(3), &[]).await;

    if !events.is_empty() {
        let event = &events[0];
        let log = event.as_log();

        // Should have XML field
        assert!(
            log.contains("xml"),
            "Event should include XML when configured"
        );

        if let Some(xml) = log.get("xml") {
            let xml_str = xml.to_string_lossy();
            assert!(
                xml_str.contains("<Event"),
                "XML should contain Event element"
            );
            assert!(xml_str.contains("</Event>"), "XML should be well-formed");
        }
    }
}

/// Test Windows Event Log source with event filtering
#[tokio::test]
async fn test_windows_eventlog_event_filtering() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        connection_timeout_secs: 30,
        event_timeout_ms: 5000,
        batch_size: 10,
        // Filter for specific event types
        only_event_ids: Some(vec![1000, 1001, 1002, 7036, 7040]), // Service events
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(3), &[]).await;

    // Verify filtering worked (may be empty if no matching events)
    for event in events.iter() {
        if let Some(event_id) = event.as_log().get("event_id") {
            let id = event_id.to_string_lossy().parse::<u32>().unwrap_or(0);
            assert!(
                [1000, 1001, 1002, 7036, 7040].contains(&id),
                "Event ID {} should be in allowed list",
                id
            );
        }
    }
}

/// Test Windows Event Log source resilience
#[tokio::test]
async fn test_windows_eventlog_resilience() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string(), "Application".to_string()],
        connection_timeout_secs: 5, // Shorter timeout for resilience test
        event_timeout_ms: 1000,     // Short timeout
        batch_size: 5,
        ..Default::default()
    };

    // This test verifies the source can handle short timeouts gracefully
    let events = run_and_assert_source_compliance(config, Duration::from_secs(2), &[]).await;

    // Even with short timeouts, source should not crash
    println!("Received {} events during resilience test", events.len());
}

/// Test Windows Event Log source with field filtering
#[tokio::test]
async fn test_windows_eventlog_field_filtering() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        connection_timeout_secs: 30,
        event_timeout_ms: 5000,
        batch_size: 5,
        field_filter: FieldFilter {
            include_system_fields: true,
            include_event_data: false, // Exclude event data
            include_user_data: false,  // Exclude user data
            include_fields: Some(vec![
                "event_id".to_string(),
                "level".to_string(),
                "timestamp".to_string(),
                "channel".to_string(),
            ]),
            exclude_fields: Some(vec!["raw_xml".to_string()]),
        },
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(3), &[]).await;

    if !events.is_empty() {
        let event = &events[0];
        let log = event.as_log();

        // Should have included fields
        assert!(log.contains("event_id"), "Should include event_id");
        assert!(log.contains("level"), "Should include level");
        assert!(log.contains("timestamp"), "Should include timestamp");
        assert!(log.contains("channel"), "Should include channel");

        // Should not have excluded fields
        assert!(!log.contains("raw_xml"), "Should exclude raw_xml");
        assert!(
            !log.contains("event_data"),
            "Should exclude event_data when configured"
        );
    }
}

/// Test Windows Event Log source namespace handling
#[tokio::test]
async fn test_windows_eventlog_namespaces() {
    // Test legacy namespace
    let mut config = WindowsEventLogConfig {
        channels: vec!["Application".to_string()],
        connection_timeout_secs: 30,
        event_timeout_ms: 5000,
        batch_size: 3,
        log_namespace: Some(false), // Legacy namespace
        ..Default::default()
    };

    let events =
        run_and_assert_source_compliance(config.clone(), Duration::from_secs(2), &[]).await;

    if !events.is_empty() {
        let event = &events[0];
        // Legacy namespace structure validation would go here
        assert!(event.as_log().contains("timestamp"));
    }

    // Test vector namespace
    config.log_namespace = Some(true);
    let events = run_and_assert_source_compliance(config, Duration::from_secs(2), &[]).await;

    if !events.is_empty() {
        let event = &events[0];
        // Vector namespace structure validation would go here
        assert!(event.as_log().contains("timestamp"));
    }
}

/// Test Windows Event Log source error handling
#[tokio::test]
async fn test_windows_eventlog_error_handling() {
    // Test with invalid channel (should handle gracefully)
    let config = WindowsEventLogConfig {
        channels: vec!["NonExistentChannel".to_string()],
        connection_timeout_secs: 5,
        event_timeout_ms: 1000,
        batch_size: 5,
        ..Default::default()
    };

    // This should not panic, even with invalid channel
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let _ = run_and_assert_source_compliance(config, Duration::from_secs(1), &[]).await;
        });
    }));

    // Source should handle invalid channels gracefully without panicking
    assert!(
        result.is_ok(),
        "Source should handle invalid channels gracefully"
    );
}

/// Performance test for Windows Event Log source
#[tokio::test]
async fn test_windows_eventlog_performance() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string(), "Application".to_string()],
        connection_timeout_secs: 30,
        event_timeout_ms: 5000,
        batch_size: 50,             // Larger batch size for performance test
        read_existing_events: true, // Read existing events to ensure we have data
        ..Default::default()
    };

    let start_time = std::time::Instant::now();
    let events = run_and_assert_source_compliance(config, Duration::from_secs(5), &[]).await;
    let elapsed = start_time.elapsed();

    println!(
        "Performance test: {} events in {:?} ({:.2} events/sec)",
        events.len(),
        elapsed,
        events.len() as f64 / elapsed.as_secs_f64()
    );

    // Basic performance assertion - should process at least some events
    if events.len() > 10 {
        // If we have enough events, verify reasonable performance
        let events_per_second = events.len() as f64 / elapsed.as_secs_f64();
        assert!(
            events_per_second > 1.0,
            "Should process at least 1 event per second, got {:.2}",
            events_per_second
        );
    }
}

/// Test Windows Event Log source with real-time event generation
#[cfg(windows)]
#[tokio::test]
async fn test_windows_eventlog_real_time_events() {
    use std::process::Command;

    let config = WindowsEventLogConfig {
        channels: vec!["Application".to_string()],
        connection_timeout_secs: 30,
        event_timeout_ms: 5000,
        batch_size: 10,
        read_existing_events: false, // Only new events
        ..Default::default()
    };

    // Start the source
    let mut runner = SourceTestRunner::new(config);

    // Generate a test event using Windows eventcreate command
    let test_id = random_string(8);
    let test_message = format!("Vector integration test event {}", test_id);

    let output = Command::new("eventcreate")
        .args(&[
            "/T",
            "INFORMATION",
            "/ID",
            "1001",
            "/L",
            "APPLICATION",
            "/SO",
            "VectorTest",
            "/D",
            &test_message,
        ])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            println!("Successfully created test event");

            // Wait for the event to be processed
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Check if we received the test event
            let events = runner.collect_events_for(Duration::from_secs(3)).await;

            let found_test_event = events.iter().any(|event| {
                if let Some(message) = event.as_log().get("message") {
                    message.to_string_lossy().contains(&test_id)
                } else {
                    false
                }
            });

            if found_test_event {
                println!("Successfully detected generated test event");
            } else {
                println!("Test event not detected in {} events", events.len());
            }
        } else {
            println!(
                "Failed to create test event: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

/// Test concurrent channel processing
#[tokio::test]
async fn test_windows_eventlog_concurrent_channels() {
    let config = WindowsEventLogConfig {
        channels: vec![
            "System".to_string(),
            "Application".to_string(),
            "Security".to_string(),
        ],
        connection_timeout_secs: 30,
        event_timeout_ms: 5000,
        batch_size: 20,
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(4), &[]).await;

    // Verify we get events from different channels
    let mut channels_seen = std::collections::HashSet::new();
    for event in events.iter().take(20) {
        if let Some(channel) = event.as_log().get("channel") {
            channels_seen.insert(channel.to_string_lossy().to_string());
        }
    }

    println!("Channels seen: {:?}", channels_seen);
    // We might not see all channels depending on what events are available
    assert!(!channels_seen.is_empty(), "Should see at least one channel");
}
