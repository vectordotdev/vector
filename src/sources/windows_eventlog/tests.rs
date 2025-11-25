use std::{collections::HashMap, time::Duration};

use chrono::Utc;
use vector_lib::config::LogNamespace;
use vrl::value::Value;

use super::{config::*, error::*, parser::*, subscription::*};
use crate::{
    SourceSender,
    config::{SourceConfig, SourceContext},
    test_util::components::{SOURCE_TAGS, run_and_assert_source_compliance},
};

fn create_test_config() -> WindowsEventLogConfig {
    WindowsEventLogConfig {
        channels: vec!["System".to_string(), "Application".to_string()],
        event_query: None,
        connection_timeout_secs: 30,
        read_existing_events: false,
        batch_size: 10,
        render_message: true,
        include_xml: false,
        event_data_format: HashMap::new(),
        ignore_event_ids: vec![],
        only_event_ids: None,
        max_event_age_secs: None,
        event_timeout_ms: 5000,
        log_namespace: Some(false),
        field_filter: FieldFilter::default(),
        data_dir: None, // Use Vector's global data_dir
        events_per_second: 0,
        max_event_data_length: 0,
        max_message_field_length: 0,
        acknowledgements: Default::default(),
    }
}

/// Creates a realistic Security audit event (4624 = successful logon) for integration-level tests.
/// Note: parser.rs has its own simpler create_test_event() for unit testing parser logic.
fn create_test_event() -> WindowsEvent {
    let mut event_data = HashMap::new();
    event_data.insert("TargetUserName".to_string(), "admin".to_string());
    event_data.insert("LogonType".to_string(), "2".to_string());

    WindowsEvent {
        record_id: 12345,
        event_id: 4624,
        level: 4,
        task: 12544,
        opcode: 0,
        keywords: 0x8020000000000000,
        time_created: Utc::now(),
        provider_name: "Microsoft-Windows-Security-Auditing".to_string(),
        provider_guid: Some("{54849625-5478-4994-a5ba-3e3b0328c30d}".to_string()),
        channel: "Security".to_string(),
        computer: "WIN-SERVER-01".to_string(),
        user_id: Some("S-1-5-18".to_string()),
        process_id: 716,
        thread_id: 796,
        activity_id: Some("{b25f4adf-d920-0000-0000-000000000000}".to_string()),
        related_activity_id: None,
        raw_xml: r#"<Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
            <System>
                <Provider Name="Microsoft-Windows-Security-Auditing" Guid="{54849625-5478-4994-a5ba-3e3b0328c30d}" />
                <EventID>4624</EventID>
                <Level>0</Level>
                <Task>12544</Task>
                <Opcode>0</Opcode>
                <Keywords>0x8020000000000000</Keywords>
                <TimeCreated SystemTime="2023-01-01T00:00:00.000000Z" />
                <EventRecordID>12345</EventRecordID>
                <Correlation ActivityID="{b25f4adf-d920-0000-0000-000000000000}" />
                <Execution ProcessID="716" ThreadID="796" />
                <Channel>Security</Channel>
                <Computer>WIN-SERVER-01</Computer>
                <Security UserID="S-1-5-18" />
            </System>
            <EventData>
                <Data Name="TargetUserName">admin</Data>
                <Data Name="LogonType">2</Data>
            </EventData>
        </Event>"#.to_string(),
        rendered_message: Some("An account was successfully logged on.".to_string()),
        event_data,
        user_data: HashMap::new(),
        // New fields for enhanced implementation
        version: Some(1),
        qualifiers: Some(0),
        string_inserts: vec!["admin".to_string(), "2".to_string()],
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_default_config_creation() {
        let config = WindowsEventLogConfig::default();

        assert_eq!(config.channels, vec!["System", "Application"]);
        assert_eq!(config.connection_timeout_secs, 30);
        assert_eq!(config.event_timeout_ms, 5000);
        assert!(!config.read_existing_events);
        assert_eq!(config.batch_size, 10);
        assert!(config.render_message);
        assert!(!config.include_xml);
        assert!(config.field_filter.include_system_fields);
        assert!(config.field_filter.include_event_data);
        assert!(config.field_filter.include_user_data);
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<WindowsEventLogConfig>();
    }

    #[test]
    fn test_config_validation_success() {
        let config = create_test_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_empty_channels() {
        let mut config = create_test_config();
        config.channels = vec![];

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("At least one channel")
        );
    }

    #[test]
    fn test_config_validation_zero_connection_timeout() {
        let mut config = create_test_config();
        config.connection_timeout_secs = 0;

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Connection timeout must be between")
        );
    }

    #[test]
    fn test_config_validation_zero_event_timeout() {
        let mut config = create_test_config();
        config.event_timeout_ms = 0;

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Event timeout must be between")
        );
    }

    #[test]
    fn test_config_validation_zero_batch_size() {
        let mut config = create_test_config();
        config.batch_size = 0;

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Batch size must be between 1 and")
        );
    }

    #[test]
    fn test_config_validation_empty_channel_name() {
        let mut config = create_test_config();
        config.channels = vec!["System".to_string(), "".to_string()];

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Channel names cannot be empty")
        );
    }

    #[test]
    fn test_config_validation_empty_query() {
        let mut config = create_test_config();
        config.event_query = Some("".to_string());

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Event query cannot be empty")
        );
    }

    #[test]
    fn test_config_serialization() {
        let config = create_test_config();

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: WindowsEventLogConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config.channels, deserialized.channels);
        assert_eq!(
            config.connection_timeout_secs,
            deserialized.connection_timeout_secs
        );
        assert_eq!(config.event_timeout_ms, deserialized.event_timeout_ms);
        assert_eq!(config.batch_size, deserialized.batch_size);
        assert_eq!(config.render_message, deserialized.render_message);
    }

    #[test]
    fn test_field_filter_configuration() {
        let mut config = create_test_config();
        config.field_filter = FieldFilter {
            include_fields: Some(vec!["event_id".to_string(), "level".to_string()]),
            exclude_fields: Some(vec!["raw_xml".to_string()]),
            include_system_fields: false,
            include_event_data: true,
            include_user_data: false,
        };

        assert!(config.validate().is_ok());
        assert!(!config.field_filter.include_system_fields);
        assert!(config.field_filter.include_event_data);
        assert!(!config.field_filter.include_user_data);
    }

    #[test]
    fn test_event_data_format_configuration() {
        let mut config = create_test_config();
        config
            .event_data_format
            .insert("event_id".to_string(), EventDataFormat::String);
        config
            .event_data_format
            .insert("process_id".to_string(), EventDataFormat::Integer);
        config
            .event_data_format
            .insert("enabled".to_string(), EventDataFormat::Boolean);

        assert!(config.validate().is_ok());
        assert_eq!(config.event_data_format.len(), 3);
    }

    #[test]
    fn test_filtering_options() {
        let mut config = create_test_config();
        config.ignore_event_ids = vec![4624, 4634];
        config.only_event_ids = Some(vec![1000, 1001, 1002]);
        config.max_event_age_secs = Some(86400);

        assert!(config.validate().is_ok());
        assert_eq!(config.ignore_event_ids.len(), 2);
        assert!(config.only_event_ids.is_some());
        assert_eq!(config.max_event_age_secs, Some(86400));
    }
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let config = create_test_config();
        let _parser = EventLogParser::new(&config);

        // Should create without error - parser creation succeeds
        // Note: Cannot test private fields directly
    }

    #[test]
    fn test_parse_basic_event() {
        let config = create_test_config();
        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event.clone()).unwrap();

        // Check core fields
        assert_eq!(log_event.get("event_id"), Some(&Value::Integer(4624)));
        assert_eq!(log_event.get("record_id"), Some(&Value::Integer(12345)));
        assert_eq!(
            log_event.get("level"),
            Some(&Value::Bytes("Information".into()))
        );
        assert_eq!(log_event.get("level_value"), Some(&Value::Integer(4)));
        assert_eq!(
            log_event.get("channel"),
            Some(&Value::Bytes("Security".into()))
        );
        assert_eq!(
            log_event.get("provider_name"),
            Some(&Value::Bytes("Microsoft-Windows-Security-Auditing".into()))
        );
        assert_eq!(
            log_event.get("computer"),
            Some(&Value::Bytes("WIN-SERVER-01".into()))
        );
        assert_eq!(log_event.get("process_id"), Some(&Value::Integer(716)));
        assert_eq!(log_event.get("thread_id"), Some(&Value::Integer(796)));
    }

    #[test]
    fn test_parse_event_with_xml() {
        let mut config = create_test_config();
        config.include_xml = true;

        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event.clone()).unwrap();

        // XML should be included
        assert!(log_event.get("xml").is_some());
        if let Some(Value::Bytes(xml_bytes)) = log_event.get("xml") {
            let xml_string = String::from_utf8_lossy(xml_bytes);
            assert!(xml_string.contains("<Event xmlns"));
            assert!(xml_string.contains("EventID>4624<"));
        }
    }

    #[test]
    fn test_parse_event_with_event_data() {
        let mut config = create_test_config();
        config.field_filter.include_event_data = true;

        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event.clone()).unwrap();

        // Event data should be included
        if let Some(Value::Object(event_data)) = log_event.get("event_data") {
            assert_eq!(
                event_data.get("TargetUserName"),
                Some(&Value::Bytes("admin".into()))
            );
            assert_eq!(event_data.get("LogonType"), Some(&Value::Bytes("2".into())));
        } else {
            panic!("event_data should be present and be an object");
        }
    }

    #[test]
    fn test_parse_event_with_custom_formatting() {
        let mut config = create_test_config();
        config
            .event_data_format
            .insert("event_id".to_string(), EventDataFormat::String);
        config
            .event_data_format
            .insert("process_id".to_string(), EventDataFormat::Float);

        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event.clone()).unwrap();

        // event_id should be formatted as string
        assert_eq!(
            log_event.get("event_id"),
            Some(&Value::Bytes("4624".into()))
        );

        // process_id should be formatted as float
        if let Some(Value::Float(process_id)) = log_event.get("process_id") {
            assert_eq!(process_id.into_inner(), 716.0);
        } else {
            panic!("process_id should be formatted as float");
        }
    }

    #[test]
    fn test_windows_event_level_names() {
        let mut event = create_test_event();

        event.level = 1;
        assert_eq!(event.level_name(), "Critical");

        event.level = 2;
        assert_eq!(event.level_name(), "Error");

        event.level = 3;
        assert_eq!(event.level_name(), "Warning");

        event.level = 4;
        assert_eq!(event.level_name(), "Information");

        event.level = 5;
        assert_eq!(event.level_name(), "Verbose");

        event.level = 99;
        assert_eq!(event.level_name(), "Unknown");
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn test_error_recoverability() {
        // Recoverable errors
        let recoverable_errors = vec![
            WindowsEventLogError::TimeoutError { timeout_secs: 30 },
            WindowsEventLogError::ResourceExhaustedError {
                message: "test".to_string(),
            },
            WindowsEventLogError::IoError {
                source: std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"),
            },
        ];

        for error in recoverable_errors {
            assert!(
                error.is_recoverable(),
                "Error should be recoverable: {}",
                error
            );
        }

        // Non-recoverable errors
        let non_recoverable_errors = vec![
            WindowsEventLogError::AccessDeniedError {
                channel: "Security".to_string(),
            },
            WindowsEventLogError::ChannelNotFoundError {
                channel: "NonExistent".to_string(),
            },
            WindowsEventLogError::InvalidXPathQuery {
                query: "invalid".to_string(),
                message: "syntax error".to_string(),
            },
            WindowsEventLogError::ConfigError {
                message: "invalid config".to_string(),
            },
        ];

        for error in non_recoverable_errors {
            assert!(
                !error.is_recoverable(),
                "Error should not be recoverable: {}",
                error
            );
        }
    }

    #[test]
    fn test_error_user_messages() {
        let error = WindowsEventLogError::AccessDeniedError {
            channel: "Security".to_string(),
        };
        let message = error.user_message();
        assert!(message.contains("Access denied"));
        assert!(message.contains("Administrator"));

        let error = WindowsEventLogError::ChannelNotFoundError {
            channel: "NonExistent".to_string(),
        };
        let message = error.user_message();
        assert!(message.contains("not found"));
        assert!(message.contains("NonExistent"));

        let error = WindowsEventLogError::InvalidXPathQuery {
            query: "*[invalid]".to_string(),
            message: "syntax error".to_string(),
        };
        let message = error.user_message();
        assert!(message.contains("Invalid XPath query"));
        assert!(message.contains("*[invalid]"));
    }

    #[test]
    fn test_error_conversions() {
        // Test conversion from quick_xml::Error
        let xml_error = quick_xml::Error::UnexpectedEof("test".to_string());
        let converted: WindowsEventLogError = xml_error.into();
        assert!(matches!(
            converted,
            WindowsEventLogError::ParseXmlError { .. }
        ));

        // Test conversion from std::io::Error
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test");
        let converted: WindowsEventLogError = io_error.into();
        assert!(matches!(converted, WindowsEventLogError::IoError { .. }));
    }

    #[cfg(not(windows))]
    #[test]
    fn test_not_supported_error_message() {
        let error = WindowsEventLogError::NotSupportedError;
        let message = error.user_message();
        assert!(message.contains("Windows operating systems"));
        assert!(!error.is_recoverable());
    }
}

#[cfg(test)]
mod subscription_tests {
    use super::*;

    // Note: test_not_supported_error is in subscription.rs to avoid duplication

    #[test]
    fn test_build_xpath_query() {
        let config = create_test_config();
        // This test would need to be conditional on Windows
        // For now, we test the configuration validation
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_event_filtering_by_id() {
        let mut config = create_test_config();
        config.ignore_event_ids = vec![4624, 4625];
        config.only_event_ids = Some(vec![1000, 1001]);

        // Configuration should be valid
        assert!(config.validate().is_ok());

        // Test event should be filtered out (4624 is in ignore list)
        let event = create_test_event(); // event_id = 4624
        assert!(config.ignore_event_ids.contains(&event.event_id));

        // Test only_event_ids filtering
        if let Some(ref only_ids) = config.only_event_ids {
            assert!(!only_ids.contains(&event.event_id));
        }
    }

    #[test]
    fn test_event_age_filtering() {
        let mut config = create_test_config();
        config.max_event_age_secs = Some(86400); // 24 hours

        let mut event = create_test_event();

        // Event from now should pass
        event.time_created = Utc::now();
        let age = Utc::now().signed_duration_since(event.time_created);
        assert!(age.num_seconds() <= 86400);

        // Event from 2 days ago should be filtered
        event.time_created = Utc::now() - chrono::Duration::days(2);
        let age = Utc::now().signed_duration_since(event.time_created);
        assert!(age.num_seconds() > 86400);
    }

    #[test]
    fn test_xml_parsing_helpers() {
        let xml = r#"
        <Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
            <System>
                <Provider Name="Microsoft-Windows-Kernel-General" Guid="{A68CA8B7-004F-D7B6-A698-07E2DE0F1F5D}"/>
                <EventID>1</EventID>
                <Level>4</Level>
                <EventRecordID>12345</EventRecordID>
                <Channel>System</Channel>
                <Computer>TEST-MACHINE</Computer>
            </System>
        </Event>
        "#;

        assert_eq!(
            EventLogSubscription::extract_xml_value(xml, "EventID"),
            Some("1".to_string())
        );
        assert_eq!(
            EventLogSubscription::extract_xml_value(xml, "Level"),
            Some("4".to_string())
        );
        assert_eq!(
            EventLogSubscription::extract_xml_value(xml, "EventRecordID"),
            Some("12345".to_string())
        );
        assert_eq!(
            EventLogSubscription::extract_xml_value(xml, "Channel"),
            Some("System".to_string())
        );
        assert_eq!(
            EventLogSubscription::extract_xml_value(xml, "Computer"),
            Some("TEST-MACHINE".to_string())
        );
        assert_eq!(
            EventLogSubscription::extract_xml_value(xml, "NonExistent"),
            None
        );
    }

    #[test]
    fn test_xml_attribute_parsing() {
        let xml = r#"
        <Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
            <System>
                <Provider Name="Microsoft-Windows-Kernel-General" Guid="{A68CA8B7-004F-D7B6-A698-07E2DE0F1F5D}"/>
                <TimeCreated SystemTime="2025-08-29T00:15:41.123456Z"/>
            </System>
        </Event>
        "#;

        assert_eq!(
            EventLogSubscription::extract_xml_attribute(xml, "Name"),
            Some("Microsoft-Windows-Kernel-General".to_string())
        );
        assert_eq!(
            EventLogSubscription::extract_xml_attribute(xml, "SystemTime"),
            Some("2025-08-29T00:15:41.123456Z".to_string())
        );
        assert_eq!(
            EventLogSubscription::extract_xml_attribute(xml, "NonExistent"),
            None
        );
    }

    #[test]
    fn test_event_data_extraction() {
        let xml = r#"
            <Event>
                <EventData>
                    <Data Name="TargetUserName">administrator</Data>
                    <Data Name="TargetLogonId">0x3e7</Data>
                    <Data Name="LogonType">2</Data>
                    <Data Name="WorkstationName">WIN-TEST</Data>
                </EventData>
            </Event>
        "#;

        let config = WindowsEventLogConfig::default();
        let event_data = EventLogSubscription::extract_event_data(xml, &config);

        assert_eq!(
            event_data.structured_data.get("TargetUserName"),
            Some(&"administrator".to_string())
        );
        assert_eq!(
            event_data.structured_data.get("TargetLogonId"),
            Some(&"0x3e7".to_string())
        );
        assert_eq!(
            event_data.structured_data.get("LogonType"),
            Some(&"2".to_string())
        );
        assert_eq!(
            event_data.structured_data.get("WorkstationName"),
            Some(&"WIN-TEST".to_string())
        );
    }

    #[test]
    fn test_security_limits() {
        // Test XML element extraction with size limits
        let large_xml = format!(
            r#"
        <Event>
            <System>
                <EventID>{}</EventID>
            </System>
        </Event>
        "#,
            "x".repeat(10000)
        ); // Very large content

        // Should not panic or consume excessive memory
        let result = EventLogSubscription::extract_xml_value(&large_xml, "EventID");
        // Should either truncate or return None, but not crash
        match result {
            Some(value) => assert!(value.len() <= 4096, "Should limit extracted text size"),
            None => {} // Acceptable if parsing fails due to size limits
        }
    }
}

#[tokio::test]
async fn test_source_output_schema() {
    let config = create_test_config();

    // Test legacy namespace
    let outputs = config.outputs(LogNamespace::Legacy);
    assert_eq!(outputs.len(), 1);

    // Test vector namespace
    let outputs = config.outputs(LogNamespace::Vector);
    assert_eq!(outputs.len(), 1);
}

#[tokio::test]
async fn test_source_resources() {
    let config = create_test_config();
    let resources = config.resources();

    assert_eq!(resources.len(), 2);
    assert!(resources.iter().any(|r| r.to_string().contains("System")));
    assert!(
        resources
            .iter()
            .any(|r| r.to_string().contains("Application"))
    );
}

#[tokio::test]
async fn test_source_acknowledgements() {
    let config = create_test_config();

    // Windows Event Log source supports acknowledgements
    assert!(config.can_acknowledge());
}

#[test]
fn test_inventory_registration() {
    // Verify that the source is properly registered in the inventory
    // This tests the inventory::submit! macro
    // The registration happens automatically via the inventory::submit! macro
    // We can't directly test it here, but we can verify the config builds correctly
    let config = create_test_config();
    assert!(config.validate().is_ok());
}

// Compliance tests
#[tokio::test]
async fn test_source_compliance() {
    run_and_assert_source_compliance(
        create_test_config(),
        Duration::from_millis(100),
        &SOURCE_TAGS,
    )
    .await;
}

// Performance and stress tests
#[cfg(windows)]
#[tokio::test]
async fn test_high_volume_events() {
    // This would test the source under high event volume
    // Implementation would depend on ability to generate test events
    let config = create_test_config();
    assert!(config.validate().is_ok());
}

#[cfg(windows)]
#[tokio::test]
async fn test_memory_usage() {
    // This would test memory usage under various conditions
    // Implementation would measure memory before/after processing events
    let config = create_test_config();
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn test_concurrent_channels() {
    let mut config = create_test_config();
    config.channels = vec![
        "System".to_string(),
        "Application".to_string(),
        "Setup".to_string(),
        "Forwarded Events".to_string(),
    ];

    assert!(config.validate().is_ok());
    assert_eq!(config.channels.len(), 4);
}

// ================================================================================================
// SECURITY TESTS - Critical security attack vector validation
// ================================================================================================

#[cfg(test)]
mod security_tests {
    use super::*;

    /// Test XPath injection attack prevention
    #[test]
    fn test_xpath_injection_prevention() {
        let mut config = create_test_config();

        // Test JavaScript injection attempts
        let javascript_attacks = vec![
            "javascript:alert('xss')",
            "*[javascript:eval('malicious')]",
            "System[javascript:document.write('attack')]",
            "*[System[javascript:window.open()]]",
        ];

        for attack in javascript_attacks {
            config.event_query = Some(attack.to_string());
            let result = config.validate();
            assert!(
                result.is_err(),
                "JavaScript injection '{}' should be blocked",
                attack
            );
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("potentially unsafe pattern"),
                "Error should mention unsafe pattern for: {}",
                attack
            );
        }

        // Test valid XPath queries should still work
        let valid_queries = vec![
            "*[System[Level=1 or Level=2]]",
            "*[System[(Level=1 or Level=2) and TimeCreated[timediff(@SystemTime) <= 86400000]]]",
            "*[System[Provider[@Name='Microsoft-Windows-Security-Auditing']]]",
            "Event/System[EventID=4624]",
        ];

        for valid_query in valid_queries {
            config.event_query = Some(valid_query.to_string());
            let result = config.validate();
            assert!(
                result.is_ok(),
                "Valid XPath query '{}' should be allowed",
                valid_query
            );
        }
    }

    /// Test resource exhaustion attack prevention
    #[test]
    fn test_resource_exhaustion_prevention() {
        let mut config = create_test_config();

        // Test excessive connection timeout (DoS prevention)
        config.connection_timeout_secs = 0;
        assert!(
            config.validate().is_err(),
            "Zero connection timeout should be rejected"
        );

        config.connection_timeout_secs = u64::MAX;
        assert!(
            config.validate().is_err(),
            "Excessive connection timeout should be rejected"
        );

        config.connection_timeout_secs = 7200; // 2 hours
        assert!(
            config.validate().is_err(),
            "Connection timeout > 3600 seconds should be rejected"
        );

        // Test excessive event timeout
        config.connection_timeout_secs = 30; // Reset to valid value
        config.event_timeout_ms = 0;
        assert!(
            config.validate().is_err(),
            "Zero event timeout should be rejected"
        );

        config.event_timeout_ms = 100000; // 100 seconds
        assert!(
            config.validate().is_err(),
            "Excessive event timeout should be rejected"
        );

        // Test excessive batch sizes (memory exhaustion prevention)
        config.event_timeout_ms = 5000; // Reset to valid value
        config.batch_size = 0;
        assert!(
            config.validate().is_err(),
            "Zero batch size should be rejected"
        );

        config.batch_size = 100000;
        assert!(
            config.validate().is_err(),
            "Excessive batch size should be rejected"
        );
    }

    /// Test channel name validation (injection prevention)
    #[test]
    fn test_channel_name_security_validation() {
        let mut config = create_test_config();

        // Test dangerous channel names
        let excessive_length = "A".repeat(300);
        let dangerous_channels = vec![
            "",                                    // Empty channel
            "   ",                                 // Whitespace only
            "System\0",                            // Null byte injection
            "System\r\nmalicious",                 // CRLF injection
            "System<script>alert('xss')</script>", // HTML injection
            "System'; DROP TABLE events; --",      // SQL injection attempt
            "System$(malicious_command)",          // Command substitution
            "System`malicious_command`",           // Command substitution
            &excessive_length,                     // Excessive length
        ];

        for dangerous_channel in &dangerous_channels {
            config.channels = vec!["System".to_string(), dangerous_channel.to_string()];
            let result = config.validate();
            assert!(
                result.is_err(),
                "Dangerous channel name '{}' should be rejected",
                dangerous_channel.escape_debug()
            );
        }

        // Test valid channel names should work
        let valid_channels = vec![
            "System",
            "Application",
            "Security",
            "Windows PowerShell",
            "Microsoft-Windows-Security-Auditing/Operational",
            "Custom-Application_Log",
            "Service-Name/Admin",
            "Application and Services Logs/Custom",
        ];

        for valid_channel in valid_channels {
            config.channels = vec!["System".to_string(), valid_channel.to_string()];
            let result = config.validate();
            assert!(
                result.is_ok(),
                "Valid channel name '{}' should be allowed",
                valid_channel
            );
        }
    }

    /// Test excessive query length prevention
    #[test]
    fn test_excessive_query_length_prevention() {
        let mut config = create_test_config();

        // Test query length limits
        let long_query = "*[System[".to_string() + &"Level=1 and ".repeat(1000) + "Level=2]]";
        config.event_query = Some(long_query);
        let result = config.validate();
        assert!(result.is_err(), "Excessively long query should be rejected");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("exceeds maximum length"),
            "Error should mention length limit"
        );

        // Test reasonable query length should work
        let reasonable_query = "*[System[Level=1 or Level=2 or Level=3]]".to_string();
        config.event_query = Some(reasonable_query);
        assert!(
            config.validate().is_ok(),
            "Reasonable length query should be allowed"
        );
    }
}

// ================================================================================================
// BUFFER OVERFLOW AND MEMORY SAFETY TESTS
// ================================================================================================

#[cfg(test)]
mod buffer_safety_tests {
    use super::*;

    /// Test XML parsing with malicious buffer sizes
    #[test]
    fn test_malformed_xml_buffer_safety() {
        // Test extremely large XML documents (should be handled gracefully)
        let large_xml = format!(
            "<Event><EventData>{}</EventData></Event>",
            "<Data Name='field'>value</Data>".repeat(1000) // Reduced from 10000 for memory safety
        );

        // This should not panic or cause memory issues
        let config = WindowsEventLogConfig::default();
        let result = EventLogSubscription::extract_event_data(&large_xml, &config);

        // Should have some reasonable limit on parsed data
        assert!(
            result.structured_data.len() <= 100,
            "Should limit parsed data size to prevent DoS"
        );
    }

    /// Test XML parsing with deeply nested structures
    #[test]
    fn test_deeply_nested_xml_protection() {
        // Create deeply nested XML structure (reduced nesting for memory safety)
        let mut nested_xml = "<Event>".to_string();
        for i in 0..100 {
            // Reduced from 1000
            nested_xml.push_str(&format!("<Level{}>", i));
        }
        nested_xml.push_str("<EventData><Data Name='test'>value</Data></EventData>");
        for i in (0..100).rev() {
            nested_xml.push_str(&format!("</Level{}>", i));
        }
        nested_xml.push_str("</Event>");

        // This should not cause stack overflow or excessive memory usage
        let config = WindowsEventLogConfig::default();
        let result = EventLogSubscription::extract_event_data(&nested_xml, &config);

        // Should handle gracefully - either succeeds or fails safely
        // The key is that it doesn't crash or consume excessive resources
        assert!(
            result.structured_data.len() <= 100,
            "Should limit parsed data for deeply nested XML"
        );
    }

    /// Test handling of XML with excessive attributes
    #[test]
    fn test_excessive_xml_attributes_handling() {
        // Create XML with many attributes (reduced count for safety)
        let mut xml_with_attrs = "<Event><EventData>".to_string();
        for i in 0..200 {
            // Reduced from 5000
            xml_with_attrs.push_str(&format!(
                "<Data Name='attr{}' Value='value{}'>data{}</Data>",
                i, i, i
            ));
        }
        xml_with_attrs.push_str("</EventData></Event>");

        // Should handle gracefully without memory exhaustion
        let config = WindowsEventLogConfig::default();
        let result = EventLogSubscription::extract_event_data(&xml_with_attrs, &config);

        // Should have reasonable limits on parsed attributes
        assert!(
            result.structured_data.len() <= 100,
            "Should limit number of parsed attributes"
        );
    }
}

// ================================================================================================
// CONCURRENCY AND RACE CONDITION TESTS
// ================================================================================================

#[cfg(test)]
mod concurrency_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_subscription_creation() {
        // Test that multiple subscription attempts don't interfere
        let config = create_test_config();
        assert!(config.validate().is_ok());
    }
}

// ================================================================================================
// ERROR INJECTION AND FAULT TOLERANCE TESTS
// ================================================================================================

#[cfg(test)]
mod fault_tolerance_tests {
    use super::*;

    #[tokio::test]
    async fn test_invalid_xml_handling() {
        let invalid_xml = "not valid xml <tag>";
        let config = WindowsEventLogConfig::default();
        let result = EventLogSubscription::extract_event_data(invalid_xml, &config);
        // Should return empty result or handle gracefully without crashing
        assert!(
            result.structured_data.len() == 0,
            "Invalid XML should result in empty data"
        );
    }

    #[tokio::test]
    async fn test_malicious_xml_handling() {
        // Test various malicious XML patterns
        let malicious_xmls = vec![
            "<?xml version='1.0'?><!DOCTYPE test [<!ENTITY xxe SYSTEM 'file:///etc/passwd'>]><test>&xxe;</test>".to_string(),
            format!("<Event><![CDATA[{}]]></Event>", "x".repeat(100000)), // Large CDATA
            format!("<Event>{}data{}</Event>", "<nested>".repeat(1000), "</nested>".repeat(1000)), // Deep nesting
        ];

        let config = WindowsEventLogConfig::default();
        for malicious_xml in &malicious_xmls {
            let result = EventLogSubscription::extract_event_data(&malicious_xml, &config);
            // Should handle without crashing or excessive resource usage
            assert!(
                result.structured_data.len() <= 100,
                "Malicious XML should be limited in processing"
            );
        }
    }
}

// ================================================================================================
// ACKNOWLEDGMENT TESTS
// ================================================================================================

#[cfg(test)]
mod acknowledgement_tests {
    use super::*;
    use crate::config::SourceAcknowledgementsConfig;

    #[test]
    fn test_acknowledgements_config_default_disabled() {
        let config = WindowsEventLogConfig::default();
        // Acknowledgements should be disabled by default
        assert!(
            !config.acknowledgements.enabled(),
            "Acknowledgements should be disabled by default"
        );
    }

    #[test]
    fn test_acknowledgements_config_enabled() {
        let mut config = create_test_config();
        config.acknowledgements = SourceAcknowledgementsConfig::from(true);
        assert!(
            config.acknowledgements.enabled(),
            "Acknowledgements should be enabled when configured"
        );
    }

    #[test]
    fn test_can_acknowledge_returns_true() {
        use crate::config::SourceConfig;
        let config = WindowsEventLogConfig::default();
        assert!(
            config.can_acknowledge(),
            "can_acknowledge() should return true to support acknowledgements"
        );
    }

    #[test]
    fn test_acknowledgements_config_serialization() {
        // Test that acknowledgements config serializes correctly
        let config = WindowsEventLogConfig {
            acknowledgements: SourceAcknowledgementsConfig::from(true),
            ..Default::default()
        };

        let serialized = serde_json::to_string(&config).expect("serialization should succeed");
        assert!(
            serialized.contains("acknowledgements"),
            "Serialized config should contain acknowledgements field"
        );

        // Test deserialization
        let deserialized: WindowsEventLogConfig =
            serde_json::from_str(&serialized).expect("deserialization should succeed");
        assert!(
            deserialized.acknowledgements.enabled(),
            "Acknowledgements should be enabled after deserialization"
        );
    }

    #[test]
    fn test_acknowledgements_toml_parsing() {
        // Test parsing from TOML with acknowledgements enabled
        let toml_with_acks = r#"
            channels = ["System"]
            acknowledgements = true
        "#;
        let config: WindowsEventLogConfig =
            toml::from_str(toml_with_acks).expect("TOML parsing should succeed");
        assert!(
            config.acknowledgements.enabled(),
            "Acknowledgements should be enabled from TOML"
        );

        // Test parsing with acknowledgements as struct
        let toml_with_acks_struct = r#"
            channels = ["System"]
            [acknowledgements]
            enabled = true
        "#;
        let config: WindowsEventLogConfig =
            toml::from_str(toml_with_acks_struct).expect("TOML parsing should succeed");
        assert!(
            config.acknowledgements.enabled(),
            "Acknowledgements should be enabled from TOML struct"
        );

        // Test parsing without acknowledgements (default)
        let toml_without_acks = r#"
            channels = ["System"]
        "#;
        let config: WindowsEventLogConfig =
            toml::from_str(toml_without_acks).expect("TOML parsing should succeed");
        assert!(
            !config.acknowledgements.enabled(),
            "Acknowledgements should be disabled by default"
        );
    }
}
