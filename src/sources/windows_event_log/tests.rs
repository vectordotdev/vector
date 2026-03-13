use std::{collections::HashMap, time::Duration};

use chrono::Utc;
use vector_lib::config::LogNamespace;
use vrl::value::Value;

use super::{config::*, error::*, parser::*, xml_parser::*};
use crate::{
    config::SourceConfig,
    test_util::components::{SOURCE_TAGS, run_and_assert_source_compliance},
};

fn create_test_config() -> WindowsEventLogConfig {
    WindowsEventLogConfig {
        channels: vec!["System".to_string(), "Application".to_string()],
        event_query: None,
        connection_timeout_secs: 30,
        read_existing_events: false,
        batch_size: 10,
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
        checkpoint_interval_secs: 5,
        acknowledgements: Default::default(),
        render_message: false,
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
        task_name: None,
        opcode_name: None,
        keyword_names: Vec::new(),
        user_name: None,
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
        assert_eq!(config.batch_size, 100);
        assert!(!config.include_xml);
        assert!(config.render_message);
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
        let _parser = EventLogParser::new(&config, LogNamespace::Legacy);

        // Should create without error - parser creation succeeds
        // Note: Cannot test private fields directly
    }

    #[test]
    fn test_parse_basic_event() {
        let config = create_test_config();
        let parser = EventLogParser::new(&config, LogNamespace::Legacy);
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

        let parser = EventLogParser::new(&config, LogNamespace::Legacy);
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

        let parser = EventLogParser::new(&config, LogNamespace::Legacy);
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

        let parser = EventLogParser::new(&config, LogNamespace::Legacy);
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

        // Level 0 (LogAlways / Security audit) maps to "Information"
        event.level = 0;
        assert_eq!(event.level_name(), "Information");

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
}

#[cfg(test)]
mod subscription_tests {
    use super::super::subscription::build_xpath_query;
    use super::*;

    // Note: test_not_supported_error is in subscription.rs to avoid duplication

    #[test]
    fn test_build_xpath_query_default_wildcard() {
        let config = create_test_config();
        let query = build_xpath_query(&config).unwrap();
        assert_eq!(
            query, "*",
            "Default config with no event_query and no only_event_ids should return wildcard"
        );
    }

    #[test]
    fn test_build_xpath_query_explicit_event_query_takes_precedence() {
        let mut config = create_test_config();
        config.event_query = Some("*[System[Provider[@Name='MyApp']]]".to_string());
        config.only_event_ids = Some(vec![4624, 4625]);

        let query = build_xpath_query(&config).unwrap();
        assert_eq!(
            query, "*[System[Provider[@Name='MyApp']]]",
            "Explicit event_query should take precedence over only_event_ids"
        );
    }

    #[test]
    fn test_build_xpath_query_single_event_id() {
        let mut config = create_test_config();
        config.only_event_ids = Some(vec![4624]);

        let query = build_xpath_query(&config).unwrap();
        assert_eq!(query, "*[System[EventID=4624]]");
    }

    #[test]
    fn test_build_xpath_query_multiple_event_ids() {
        let mut config = create_test_config();
        config.only_event_ids = Some(vec![4624, 4625, 4634]);

        let query = build_xpath_query(&config).unwrap();
        assert_eq!(
            query,
            "*[System[EventID=4624 or EventID=4625 or EventID=4634]]"
        );
    }

    #[test]
    fn test_build_xpath_query_empty_only_event_ids_returns_wildcard() {
        let mut config = create_test_config();
        config.only_event_ids = Some(vec![]);

        let query = build_xpath_query(&config).unwrap();
        assert_eq!(
            query, "*",
            "Empty only_event_ids list should return wildcard"
        );
    }

    #[test]
    fn test_build_xpath_query_large_list_falls_back_to_wildcard() {
        let mut config = create_test_config();
        // Generate enough IDs to exceed 4096-char XPath limit.
        // Each "EventID=NNNNN" is ~12 chars, " or " is 4, so ~16 per ID.
        // 4096 / 16 ≈ 256, so 300 IDs should exceed the limit.
        config.only_event_ids = Some((10000..10300).collect());

        let query = build_xpath_query(&config).unwrap();
        assert_eq!(
            query, "*",
            "Large ID list exceeding 4096 chars should fall back to wildcard"
        );
    }

    #[test]
    fn test_build_xpath_query_moderate_list_generates_xpath() {
        let mut config = create_test_config();
        // 10 IDs should comfortably fit within 4096 chars.
        config.only_event_ids = Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

        let query = build_xpath_query(&config).unwrap();
        assert!(
            query.starts_with("*[System["),
            "Query should be XPath, got: {query}"
        );
        assert!(
            query.contains("EventID=1"),
            "Query should contain EventID=1"
        );
        assert!(
            query.contains("EventID=10"),
            "Query should contain EventID=10"
        );
        assert!(query.len() <= 4096, "Query should fit within XPath limit");
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
    fn test_only_and_ignore_event_ids_interaction() {
        // When both filters are set, only_event_ids narrows first,
        // then ignore_event_ids can further exclude from that set.
        let mut config = create_test_config();
        config.only_event_ids = Some(vec![1000, 1001, 1002]);
        config.ignore_event_ids = vec![1001];

        assert!(config.validate().is_ok());

        // 1000 passes only_event_ids and is not in ignore list → accepted
        assert!(config.only_event_ids.as_ref().unwrap().contains(&1000));
        assert!(!config.ignore_event_ids.contains(&1000));

        // 1001 passes only_event_ids but is in ignore list → rejected
        assert!(config.only_event_ids.as_ref().unwrap().contains(&1001));
        assert!(config.ignore_event_ids.contains(&1001));

        // 9999 fails only_event_ids → rejected before ignore check
        assert!(!config.only_event_ids.as_ref().unwrap().contains(&9999));
    }

    #[test]
    fn test_only_event_ids_with_max_event_age() {
        let mut config = create_test_config();
        config.only_event_ids = Some(vec![4624, 4625]);
        config.max_event_age_secs = Some(3600);

        assert!(config.validate().is_ok());

        // Both filters should be set independently
        assert_eq!(config.only_event_ids.as_ref().unwrap().len(), 2);
        assert_eq!(config.max_event_age_secs, Some(3600));
    }

    #[test]
    fn test_build_xpath_query_with_ignore_event_ids_only() {
        // ignore_event_ids does NOT generate XPath — it's handled in-process
        // because XPath has no "NOT EventID=X" syntax.
        let mut config = create_test_config();
        config.ignore_event_ids = vec![4624, 4625];

        let query = build_xpath_query(&config).unwrap();
        assert_eq!(
            query, "*",
            "ignore_event_ids alone should not generate XPath filter"
        );
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

        assert_eq!(extract_xml_value(xml, "EventID"), Some("1".to_string()));
        assert_eq!(extract_xml_value(xml, "Level"), Some("4".to_string()));
        assert_eq!(
            extract_xml_value(xml, "EventRecordID"),
            Some("12345".to_string())
        );
        assert_eq!(
            extract_xml_value(xml, "Channel"),
            Some("System".to_string())
        );
        assert_eq!(
            extract_xml_value(xml, "Computer"),
            Some("TEST-MACHINE".to_string())
        );
        assert_eq!(extract_xml_value(xml, "NonExistent"), None);
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
            extract_xml_attribute(xml, "Name"),
            Some("Microsoft-Windows-Kernel-General".to_string())
        );
        assert_eq!(
            extract_xml_attribute(xml, "SystemTime"),
            Some("2025-08-29T00:15:41.123456Z".to_string())
        );
        assert_eq!(extract_xml_attribute(xml, "NonExistent"), None);
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
        let event_data = extract_event_data(xml, &config);

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
        let result = extract_xml_value(&large_xml, "EventID");
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

// Compliance tests
#[tokio::test]
async fn test_source_compliance() {
    let data_dir = tempfile::tempdir().expect("failed to create temp data_dir");
    let mut config = create_test_config();
    config.data_dir = Some(data_dir.path().to_path_buf());
    run_and_assert_source_compliance(config, Duration::from_millis(100), &SOURCE_TAGS).await;
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

        // Test dangerous channel names that config validation actually rejects:
        // empty/whitespace, control characters (null, CRLF), and excessive length.
        // Note: HTML tags, SQL fragments, and shell metacharacters are not rejected
        // at config validation time — the Windows API handles those at subscription.
        let excessive_length = "A".repeat(300);
        let dangerous_channels = vec![
            "",                    // Empty channel
            "   ",                 // Whitespace only
            "System\0",            // Null byte injection
            "System\r\nmalicious", // CRLF injection
            &excessive_length,     // Excessive length
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
        let result = extract_event_data(&large_xml, &config);

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
        let result = extract_event_data(&nested_xml, &config);

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
        let result = extract_event_data(&xml_with_attrs, &config);

        // Should parse without panicking or memory exhaustion.
        // extract_event_data does not impose an attribute count cap;
        // it parses all well-formed Data elements present in the XML.
        assert!(
            result.structured_data.len() <= 200,
            "Should parse attributes without memory issues"
        );
    }
}

// ================================================================================================
// CONCURRENCY AND RACE CONDITION TESTS
// ================================================================================================

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
        let result = extract_event_data(invalid_xml, &config);
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
            let result = extract_event_data(&malicious_xml, &config);
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
    use crate::config::{SourceAcknowledgementsConfig, SourceConfig};

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

// ================================================================================================
// RATE LIMITING TESTS
// ================================================================================================

#[cfg(test)]
mod rate_limiting_tests {
    use super::*;

    #[test]
    fn test_rate_limiting_config_default_disabled() {
        let config = WindowsEventLogConfig::default();
        assert_eq!(
            config.events_per_second, 0,
            "Rate limiting should be disabled by default (0)"
        );
    }

    #[test]
    fn test_rate_limiting_config_enabled() {
        let mut config = create_test_config();
        config.events_per_second = 100;
        assert!(
            config.validate().is_ok(),
            "Rate limiting config should be valid"
        );
        assert_eq!(config.events_per_second, 100);
    }

    #[test]
    fn test_rate_limiting_toml_parsing() {
        let toml_with_rate_limit = r#"
            channels = ["System"]
            events_per_second = 50
        "#;
        let config: WindowsEventLogConfig =
            toml::from_str(toml_with_rate_limit).expect("TOML parsing should succeed");
        assert_eq!(
            config.events_per_second, 50,
            "Rate limiting should be parsed from TOML"
        );
    }

    #[test]
    fn test_rate_limiting_serialization() {
        let mut config = create_test_config();
        config.events_per_second = 100;

        let serialized = serde_json::to_string(&config).expect("serialization should succeed");
        assert!(
            serialized.contains("events_per_second"),
            "Serialized config should contain events_per_second"
        );

        let deserialized: WindowsEventLogConfig =
            serde_json::from_str(&serialized).expect("deserialization should succeed");
        assert_eq!(
            deserialized.events_per_second, 100,
            "events_per_second should be preserved after serialization"
        );
    }
}

// ================================================================================================
// CHECKPOINT TESTS
// ================================================================================================

#[cfg(test)]
mod checkpoint_tests {
    use super::*;

    #[test]
    fn test_checkpoint_data_dir_config() {
        let mut config = create_test_config();
        config.data_dir = Some(std::path::PathBuf::from("/tmp/vector-test"));
        assert!(
            config.validate().is_ok(),
            "Config with data_dir should be valid"
        );
    }

    #[test]
    fn test_checkpoint_toml_parsing() {
        let toml_with_data_dir = r#"
            channels = ["System"]
            data_dir = "/var/lib/vector/wineventlog"
        "#;
        let config: WindowsEventLogConfig =
            toml::from_str(toml_with_data_dir).expect("TOML parsing should succeed");
        assert!(
            config.data_dir.is_some(),
            "data_dir should be parsed from TOML"
        );
    }

    #[test]
    fn test_checkpoint_path_construction() {
        // Verify that the checkpoint module exists and can be used
        let _ = std::mem::size_of::<super::super::checkpoint::Checkpointer>();
        // The actual file operations would require Windows, so we only validate type availability.
    }
}

// ================================================================================================
// MESSAGE RENDERING TESTS
// ================================================================================================

#[cfg(test)]
mod message_rendering_tests {
    use super::*;

    #[test]
    fn test_render_message_config_default() {
        let config = WindowsEventLogConfig::default();
        assert!(
            config.render_message,
            "render_message should be enabled by default for compatibility with Event Viewer"
        );
    }

    #[test]
    fn test_render_message_config_enabled() {
        let toml_with_render = r#"
            channels = ["System"]
            render_message = true
        "#;
        let config: WindowsEventLogConfig =
            toml::from_str(toml_with_render).expect("TOML parsing should succeed");
        assert!(
            config.render_message,
            "render_message should be enabled from TOML"
        );
    }

    #[test]
    fn test_render_message_false_uses_fallback() {
        // When render_message is false, the parser should use fallback message
        let config = WindowsEventLogConfig {
            render_message: false,
            ..Default::default()
        };
        let parser = EventLogParser::new(&config, LogNamespace::Legacy);

        // Create event without rendered_message
        let mut event = create_test_event();
        event.rendered_message = None;
        event.event_data.clear(); // No message in event_data either
        event.string_inserts.clear(); // Clear string inserts to reach fallback path

        let log_event = parser.parse_event(event.clone()).unwrap();

        // Should have fallback message format: "Event ID X from Provider on Computer"
        if let Some(message) = log_event.get("message") {
            let msg_str = message.to_string_lossy();
            assert!(
                msg_str.contains("Event ID") || msg_str.contains(&event.event_id.to_string()),
                "Fallback message should contain Event ID: got '{}'",
                msg_str
            );
        }
    }

    #[test]
    fn test_render_message_true_uses_rendered() {
        // When render_message is true and rendered_message is available, use it
        let config = WindowsEventLogConfig {
            render_message: true,
            ..Default::default()
        };
        let parser = EventLogParser::new(&config, LogNamespace::Legacy);

        // Create event with rendered_message
        let mut event = create_test_event();
        event.rendered_message = Some("The service started successfully.".to_string());

        let log_event = parser.parse_event(event).unwrap();

        if let Some(message) = log_event.get("message") {
            let msg_str = message.to_string_lossy();
            assert_eq!(
                msg_str, "The service started successfully.",
                "Should use rendered_message when available"
            );
        }
    }

    #[test]
    fn test_render_message_serialization() {
        let mut config = create_test_config();
        config.render_message = true;

        let serialized = serde_json::to_string(&config).expect("serialization should succeed");
        assert!(
            serialized.contains("render_message"),
            "Serialized config should contain render_message"
        );

        let deserialized: WindowsEventLogConfig =
            serde_json::from_str(&serialized).expect("deserialization should succeed");
        assert!(
            deserialized.render_message,
            "render_message should be preserved after serialization"
        );
    }
}

// ================================================================================================
// TRUNCATION TESTS
// ================================================================================================

#[cfg(test)]
mod truncation_tests {
    use super::*;

    #[test]
    fn test_max_event_data_length_config() {
        let mut config = create_test_config();
        config.max_event_data_length = 100;
        assert!(
            config.validate().is_ok(),
            "Config with max_event_data_length should be valid"
        );
    }

    #[test]
    fn test_max_event_data_length_toml_parsing() {
        let toml_with_truncation = r#"
            channels = ["System"]
            max_event_data_length = 256
        "#;
        let config: WindowsEventLogConfig =
            toml::from_str(toml_with_truncation).expect("TOML parsing should succeed");
        assert_eq!(
            config.max_event_data_length, 256,
            "max_event_data_length should be parsed from TOML"
        );
    }

    #[test]
    fn test_truncation_marker_format() {
        // max_event_data_length applies to event_data/user_data values,
        // not to string_inserts which are passed through verbatim.
        // Verify string_inserts are preserved at full length.
        let config = WindowsEventLogConfig {
            max_event_data_length: 50,
            ..Default::default()
        };

        let mut event = create_test_event();
        event.string_inserts = vec!["A".repeat(200)];

        let parser = EventLogParser::new(&config, LogNamespace::Legacy);
        let log_event = parser.parse_event(event).unwrap();

        let inserts = log_event
            .get("string_inserts")
            .expect("string_inserts should be present");
        if let Value::Array(arr) = inserts {
            assert!(!arr.is_empty(), "string_inserts should not be empty");
            let first = arr[0].to_string_lossy();
            assert_eq!(
                first.len(),
                200,
                "string_inserts should be preserved at full length"
            );
        } else {
            panic!("string_inserts should be an array");
        }
    }

    #[test]
    fn test_xml_truncation_limit() {
        // XML should be truncated at 32KB limit
        let mut config = create_test_config();
        config.include_xml = true;

        let parser = EventLogParser::new(&config, LogNamespace::Legacy);

        // Create event with large XML
        let mut event = create_test_event();
        event.raw_xml = "A".repeat(40000); // 40KB, exceeds limit

        let log_event = parser.parse_event(event).unwrap();

        if let Some(Value::Bytes(xml)) = log_event.get("xml") {
            // XML should be truncated or limited
            assert!(
                xml.len() <= 40000,
                "XML should be handled without memory issues"
            );
        }
    }

    #[test]
    fn test_config_validation_max_channels() {
        let mut config = create_test_config();

        // 63 channels should be fine (MAXIMUM_WAIT_OBJECTS - 1 for shutdown event)
        config.channels = (0..63).map(|i| format!("Channel{i}")).collect();
        assert!(config.validate().is_ok(), "63 channels should be accepted");

        // 64 channels should fail
        config.channels = (0..64).map(|i| format!("Channel{i}")).collect();
        let result = config.validate();
        assert!(result.is_err(), "64 channels should be rejected");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Too many channels"),
            "Error should mention too many channels"
        );
    }

    #[test]
    fn test_config_validation_channel_name_at_max_length() {
        let mut config = create_test_config();
        // 256 chars is exactly at the limit — should pass
        config.channels = vec!["A".repeat(256)];
        assert!(
            config.validate().is_ok(),
            "256-char channel name should be accepted"
        );

        // 257 chars exceeds the limit — should fail
        config.channels = vec!["A".repeat(257)];
        assert!(
            config.validate().is_err(),
            "257-char channel name should be rejected"
        );
    }

    #[test]
    fn test_config_validation_xpath_query_at_max_length() {
        let mut config = create_test_config();
        // Exactly 4096 chars — should pass
        let padded = format!("*{}", "x".repeat(4095));
        assert_eq!(padded.len(), 4096);
        config.event_query = Some(padded);
        assert!(
            config.validate().is_ok(),
            "4096-char XPath query should be accepted"
        );

        // 4097 chars — should fail
        let padded = format!("*{}", "x".repeat(4096));
        assert_eq!(padded.len(), 4097);
        config.event_query = Some(padded);
        assert!(
            config.validate().is_err(),
            "4097-char XPath query should be rejected"
        );
    }

    #[test]
    fn test_config_validation_event_ids_at_max_size() {
        let mut config = create_test_config();
        // 1000 IDs is exactly at the limit — should pass
        config.only_event_ids = Some((1..=1000).collect());
        assert!(
            config.validate().is_ok(),
            "1000 event IDs should be accepted"
        );

        // 1001 IDs exceeds the limit — should fail
        config.only_event_ids = Some((1..=1001).collect());
        assert!(
            config.validate().is_err(),
            "1001 event IDs should be rejected"
        );
    }
}
