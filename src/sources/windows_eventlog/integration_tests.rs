#![cfg(feature = "sources-windows_eventlog-integration-tests")]
#![cfg(test)]

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::time::timeout;
use vector_lib::config::LogNamespace;

use super::*;
use crate::{
    config::{SourceConfig, SourceContext},
    test_util::{
        components::{run_and_assert_source_compliance, SourceTestRunner},
        random_string,
    },
};

/// Test Windows Event Log source against real Windows Event Log service
#[tokio::test]
async fn test_windows_eventlog_integration() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string(), "Application".to_string()],
        poll_interval_secs: 1,
        batch_size: 10,
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(5), &[]).await;
    assert!(!events.is_empty(), "Should receive at least one event from Windows Event Log");

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
        poll_interval_secs: 1,
        batch_size: 5,
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(3), &[]).await;
    
    // All events should be from System channel
    for event in &events {
        let log = event.as_log();
        if let Some(channel) = log.get("channel") {
            assert_eq!(channel.to_string_lossy(), "System");
        }
    }
}

/// Test Windows Event Log source with event ID filtering
#[tokio::test]
async fn test_windows_eventlog_event_id_filtering() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        poll_interval_secs: 1,
        batch_size: 5,
        ignore_event_ids: vec![1074, 6005, 6006], // Common system events to ignore
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(3), &[]).await;
    
    // Verify no ignored event IDs are present
    for event in &events {
        let log = event.as_log();
        if let Some(event_id) = log.get("event_id") {
            let id = event_id.as_integer().unwrap();
            assert!(!vec![1074, 6005, 6006].contains(&(id as u32)));
        }
    }
}

/// Test Windows Event Log source with XPath query filtering
#[tokio::test]
async fn test_windows_eventlog_xpath_filtering() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        poll_interval_secs: 1,
        batch_size: 5,
        event_query: Some("*[System/Level=2]".to_string()), // Only Error level events
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(5), &[]).await;
    
    // All events should be Error level (2)
    for event in &events {
        let log = event.as_log();
        if let Some(level) = log.get("level") {
            assert_eq!(level.to_string_lossy().to_lowercase(), "error");
        }
    }
}

/// Test Windows Event Log source bookmark persistence
#[tokio::test]
async fn test_windows_eventlog_bookmark_persistence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let bookmark_file = temp_dir.path().join("test_bookmark.xml");
    
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        poll_interval_secs: 1,
        batch_size: 5,
        bookmark_db_path: Some(bookmark_file.to_path_buf()),
        ..Default::default()
    };

    // First run - should create bookmark
    let events1 = run_and_assert_source_compliance(config.clone(), Duration::from_secs(3), &[]).await;
    assert!(!events1.is_empty());
    assert!(bookmark_file.exists(), "Bookmark file should be created");

    // Second run with same config - should resume from bookmark
    let events2 = run_and_assert_source_compliance(config, Duration::from_secs(3), &[]).await;
    
    // Verify no duplicate events (bookmark working)
    if !events1.is_empty() && !events2.is_empty() {
        let last_record_id1 = events1.last()
            .and_then(|e| e.as_log().get("record_id"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u64;
            
        let first_record_id2 = events2.first()
            .and_then(|e| e.as_log().get("record_id"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u64;
            
        assert!(first_record_id2 > last_record_id1, "Should resume after last processed event");
    }
}

/// Test Windows Event Log source error recovery
#[tokio::test]
async fn test_windows_eventlog_error_recovery() {
    let config = WindowsEventLogConfig {
        channels: vec!["NonExistentChannel".to_string()],
        poll_interval_secs: 1,
        batch_size: 5,
        ..Default::default()
    };

    // Should handle non-existent channel gracefully
    let result = config.build(SourceContext::new_test()).await;
    assert!(result.is_err(), "Should error on non-existent channel");
}

/// Test Windows Event Log source with large batch sizes
#[tokio::test]
async fn test_windows_eventlog_large_batches() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string(), "Application".to_string()],
        poll_interval_secs: 1,
        batch_size: 100, // Large batch size
        ..Default::default()
    };

    let events = run_and_assert_source_compliance(config, Duration::from_secs(10), &[]).await;
    
    // Should handle large batches without issues
    assert!(!events.is_empty());
    
    // Verify event integrity even with large batches
    for event in &events {
        let log = event.as_log();
        assert!(log.contains("timestamp"));
        assert!(log.contains("record_id"));
    }
}

/// Test Windows Event Log source performance metrics
#[tokio::test]
async fn test_windows_eventlog_metrics() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        poll_interval_secs: 1,
        batch_size: 10,
        ..Default::default()
    };

    // This test verifies that metrics are properly emitted
    // In a real scenario, you'd check internal metrics here
    let events = run_and_assert_source_compliance(config, Duration::from_secs(3), &[]).await;
    
    // Basic verification that events contain required metadata
    for event in &events {
        let log = event.as_log();
        assert!(log.contains("timestamp"));
        assert!(log.contains("record_id"));
        assert!(log.get_message().is_some());
    }
}

/// Test Windows Event Log source with custom log namespace
#[tokio::test]
async fn test_windows_eventlog_log_namespace() {
    let mut config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        poll_interval_secs: 1,
        batch_size: 5,
        log_namespace: Some(true),
        ..Default::default()
    };

    let outputs = config.outputs(LogNamespace::Vector);
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].port, None);
    
    let events = run_and_assert_source_compliance(config, Duration::from_secs(3), &[]).await;
    assert!(!events.is_empty());
}

/// Stress test Windows Event Log source with rapid polling
#[tokio::test]
async fn test_windows_eventlog_rapid_polling() {
    let config = WindowsEventLogConfig {
        channels: vec!["System".to_string()],
        poll_interval_secs: 1, // Rapid polling (minimum allowed)
        batch_size: 50,
        ..Default::default()
    };

    // Run for longer duration to test stability
    let events = run_and_assert_source_compliance(config, Duration::from_secs(10), &[]).await;
    
    // Should handle rapid polling without errors
    assert!(!events.is_empty());
    
    // Verify no duplicate record IDs
    let mut record_ids = std::collections::HashSet::new();
    for event in &events {
        if let Some(record_id) = event.as_log().get("record_id") {
            let id = record_id.as_integer().unwrap() as u64;
            assert!(record_ids.insert(id), "Found duplicate record ID: {}", id);
        }
    }
}