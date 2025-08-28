use std::{collections::HashMap, time::Duration};

use chrono::Utc;
use tokio_test;
use vector_lib::{config::LogNamespace, lookup::owned_value_path};
use vrl::value::Value;

use super::{config::*, error::*, parser::*, subscription::*};
use crate::{
    SourceSender,
    config::{SourceConfig, SourceContext},
    event::{Event, LogEvent},
    test_util::components::{SOURCE_TAGS, run_and_assert_source_compliance},
};

fn create_test_config() -> WindowsEventLogConfig {
    WindowsEventLogConfig {
        channels: vec!["System".to_string(), "Application".to_string()],
        event_query: None,
        poll_interval_secs: 1,
        read_existing_events: false,
        bookmark_db_path: None,
        batch_size: 10,
        read_limit_bytes: 524_288,
        render_message: true,
        include_xml: false,
        event_data_format: HashMap::new(),
        ignore_event_ids: vec![],
        only_event_ids: None,
        max_event_age_secs: None,
        use_subscription: true,
        log_namespace: Some(false),
        field_filter: FieldFilter::default(),
    }
}

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
        assert_eq!(config.poll_interval_secs, 1);
        assert!(!config.read_existing_events);
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.read_limit_bytes, 524_288);
        assert!(config.render_message);
        assert!(!config.include_xml);
        assert!(config.use_subscription);
        assert!(config.field_filter.include_system_fields);
        assert!(config.field_filter.include_event_data);
        assert!(config.field_filter.include_user_data);
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
    fn test_config_validation_zero_poll_interval() {
        let mut config = create_test_config();
        config.poll_interval_secs = 0;

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Poll interval must be greater than 0")
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
                .contains("Batch size must be greater than 0")
        );
    }

    #[test]
    fn test_config_validation_zero_read_limit() {
        let mut config = create_test_config();
        config.read_limit_bytes = 0;

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Read limit must be greater than 0")
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
        assert_eq!(config.poll_interval_secs, deserialized.poll_interval_secs);
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
        let parser = EventLogParser::new(&config);

        // Should create without error
        assert_eq!(parser.config.channels, config.channels);
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
    fn test_parse_event_data_xml() {
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

        let event_data = EventLogParser::parse_event_data_xml(xml).unwrap();

        assert_eq!(
            event_data.get("TargetUserName"),
            Some(&"administrator".to_string())
        );
        assert_eq!(event_data.get("TargetLogonId"), Some(&"0x3e7".to_string()));
        assert_eq!(event_data.get("LogonType"), Some(&"2".to_string()));
        assert_eq!(
            event_data.get("WorkstationName"),
            Some(&"WIN-TEST".to_string())
        );
    }

    #[test]
    fn test_format_value_conversions() {
        let config = create_test_config();
        let parser = EventLogParser::new(&config);

        // String conversion
        let result = parser
            .format_value(&Value::Integer(123), &EventDataFormat::String)
            .unwrap();
        assert_eq!(result, Value::Bytes("123".into()));

        // Integer conversion
        let result = parser
            .format_value(&Value::Bytes("456".into()), &EventDataFormat::Integer)
            .unwrap();
        assert_eq!(result, Value::Integer(456));

        // Float conversion
        let result = parser
            .format_value(&Value::Integer(789), &EventDataFormat::Float)
            .unwrap();
        if let Value::Float(f) = result {
            assert_eq!(f.into_inner(), 789.0);
        } else {
            panic!("Expected float value");
        }

        // Boolean conversion - truthy values
        let result = parser
            .format_value(&Value::Bytes("true".into()), &EventDataFormat::Boolean)
            .unwrap();
        assert_eq!(result, Value::Boolean(true));

        let result = parser
            .format_value(&Value::Integer(1), &EventDataFormat::Boolean)
            .unwrap();
        assert_eq!(result, Value::Boolean(true));

        // Boolean conversion - falsy values
        let result = parser
            .format_value(&Value::Bytes("false".into()), &EventDataFormat::Boolean)
            .unwrap();
        assert_eq!(result, Value::Boolean(false));

        let result = parser
            .format_value(&Value::Integer(0), &EventDataFormat::Boolean)
            .unwrap();
        assert_eq!(result, Value::Boolean(false));

        // Auto format (no change)
        let original = Value::Integer(999);
        let result = parser
            .format_value(&original, &EventDataFormat::Auto)
            .unwrap();
        assert_eq!(result, original);
    }

    #[test]
    fn test_format_value_error_handling() {
        let config = create_test_config();
        let parser = EventLogParser::new(&config);

        // Invalid integer conversion
        let result = parser.format_value(
            &Value::Bytes("not_a_number".into()),
            &EventDataFormat::Integer,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot convert"));

        // Invalid float conversion
        let result =
            parser.format_value(&Value::Bytes("not_a_float".into()), &EventDataFormat::Float);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot convert"));
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

        // Test conversion from rusqlite::Error
        let sqlite_error = rusqlite::Error::InvalidPath("test".into());
        let converted: WindowsEventLogError = sqlite_error.into();
        assert!(matches!(
            converted,
            WindowsEventLogError::DatabaseError { .. }
        ));
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

    #[cfg(not(windows))]
    #[test]
    fn test_subscription_not_supported() {
        let config = create_test_config();
        let result = EventLogSubscription::new(&config);

        assert!(matches!(
            result,
            Err(WindowsEventLogError::NotSupportedError)
        ));
    }

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

    // Windows Event Log source doesn't support acknowledgements
    assert!(!config.can_acknowledge());
}

// Integration test helper
#[cfg(windows)]
async fn run_source_integration_test() -> Result<(), Box<dyn std::error::Error>> {
    use crate::shutdown::ShutdownSignal;
    use tokio::sync::mpsc;

    let config = create_test_config();
    let (tx, mut rx) = mpsc::channel(100);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let context = SourceContext {
        out: SourceSender::new_test_sender(tx),
        shutdown: ShutdownSignal::new_watcher(shutdown_rx),
        ..Default::default()
    };

    // Start the source
    let source = config.build(context).await?;
    let source_handle = tokio::spawn(source);

    // Let it run for a short time
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Shutdown
    shutdown_tx.send(()).ok();
    let _ = source_handle.await;

    Ok(())
}

#[test]
fn test_inventory_registration() {
    // Verify that the source is properly registered in the inventory
    // This tests the inventory::submit! macro
    use crate::config::SourceDescription;

    // The registration happens automatically via the inventory::submit! macro
    // We can't directly test it here, but we can verify the config builds correctly
    let config = create_test_config();
    assert!(config.validate().is_ok());
}

// Compliance tests
#[tokio::test]
async fn test_source_compliance() {
    run_and_assert_source_compliance(
        &create_test_config(),
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
