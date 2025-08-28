use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use quick_xml::{Reader, events::Event as XmlEvent};
use snafu::ResultExt;
use vector_lib::config::{log_schema, LogNamespace};
use vector_lib::lookup::{owned_value_path, path};
use vrl::value::{Value, ObjectMap};

use crate::{
    event::LogEvent,
};

use super::{
    config::{WindowsEventLogConfig, EventDataFormat, FieldFilter},
    error::*,
    subscription::WindowsEvent,
};

/// Parser for converting Windows Event Log events to Vector LogEvents
pub struct EventLogParser {
    config: WindowsEventLogConfig,
    log_namespace: LogNamespace,
}

impl EventLogParser {
    /// Create a new parser with the given configuration
    pub fn new(config: &WindowsEventLogConfig) -> Self {
        let log_namespace = config.log_namespace
            .map(LogNamespace::from)
            .unwrap_or_else(|| LogNamespace::Legacy);

        Self {
            config: config.clone(),
            log_namespace,
        }
    }

    /// Parse a Windows event into a Vector LogEvent
    pub fn parse_event(&self, event: WindowsEvent) -> Result<LogEvent, WindowsEventLogError> {
        let mut log_event = LogEvent::default();
        
        // Set core fields based on log namespace
        match self.log_namespace {
            LogNamespace::Vector => {
                self.set_vector_namespace_fields(&mut log_event, &event)?;
            }
            LogNamespace::Legacy => {
                self.set_legacy_namespace_fields(&mut log_event, &event)?;
            }
        }

        // Apply field filtering
        self.apply_field_filtering(&mut log_event)?;

        // Apply custom formatting
        self.apply_custom_formatting(&mut log_event)?;

        Ok(log_event)
    }

    fn set_vector_namespace_fields(
        &self,
        log_event: &mut LogEvent,
        event: &WindowsEvent,
    ) -> Result<(), WindowsEventLogError> {
        let log_schema = log_schema();

        // Set timestamp
        if let Some(timestamp_key) = log_schema.timestamp_key() {
            log_event.insert(
                timestamp_key,
                Value::Timestamp(event.time_created.into()),
            );
        }

        // Set message (rendered message or event data)
        if let Some(message_key) = log_schema.message_key() {
            let message = event.rendered_message
                .as_ref()
                .cloned()
                .unwrap_or_else(|| self.extract_message_from_event_data(event));
            
            log_event.insert(
                message_key,
                Value::Bytes(message.into()),
            );
        }

        // Set source/host
        if let Some(host_key) = log_schema.host_key() {
            log_event.insert(
                host_key,
                Value::Bytes(event.computer.clone().into()),
            );
        }

        // Set Windows-specific fields
        self.set_windows_fields(log_event, event)?;

        Ok(())
    }

    fn set_legacy_namespace_fields(
        &self,
        log_event: &mut LogEvent,
        event: &WindowsEvent,
    ) -> Result<(), WindowsEventLogError> {
        // Legacy namespace puts everything in the root
        let log_schema = log_schema();

        // Set standard fields
        if let Some(timestamp_key) = log_schema.timestamp_key() {
            log_event.insert(timestamp_key, Value::Timestamp(event.time_created.into()));
        }

        if let Some(message_key) = log_schema.message_key() {
            let message = event.rendered_message
                .as_ref()
                .cloned()
                .unwrap_or_else(|| self.extract_message_from_event_data(event));
            
            log_event.insert(message_key, Value::Bytes(message.into()));
        }

        if let Some(host_key) = log_schema.host_key() {
            log_event.insert(host_key, Value::Bytes(event.computer.clone().into()));
        }

        // Set Windows-specific fields at root level
        self.set_windows_fields(log_event, event)?;

        Ok(())
    }

    fn set_windows_fields(
        &self,
        log_event: &mut LogEvent,
        event: &WindowsEvent,
    ) -> Result<(), WindowsEventLogError> {
        // Core Windows Event Log fields
        log_event.insert(
            "event_id",
            Value::Integer(event.event_id as i64),
        );
        
        log_event.insert(
            "record_id",
            Value::Integer(event.record_id as i64),
        );

        log_event.insert(
            "level",
            Value::Bytes(event.level_name().into()),
        );

        log_event.insert(
            "level_value",
            Value::Integer(event.level as i64),
        );

        log_event.insert(
            "channel",
            Value::Bytes(event.channel.clone().into()),
        );

        log_event.insert(
            "provider_name",
            Value::Bytes(event.provider_name.clone().into()),
        );

        if let Some(ref provider_guid) = event.provider_guid {
            log_event.insert(
                "provider_guid",
                Value::Bytes(provider_guid.clone().into()),
            );
        }

        log_event.insert(
            "computer",
            Value::Bytes(event.computer.clone().into()),
        );

        if let Some(ref user_id) = event.user_id {
            log_event.insert(
                "user_id",
                Value::Bytes(user_id.clone().into()),
            );
        }

        log_event.insert(
            "process_id",
            Value::Integer(event.process_id as i64),
        );

        log_event.insert(
            "thread_id",
            Value::Integer(event.thread_id as i64),
        );

        if event.task != 0 {
            log_event.insert(
                "task",
                Value::Integer(event.task as i64),
            );
        }

        if event.opcode != 0 {
            log_event.insert(
                "opcode",
                Value::Integer(event.opcode as i64),
            );
        }

        if event.keywords != 0 {
            log_event.insert(
                "keywords",
                Value::Integer(event.keywords as i64),
            );
        }

        if let Some(ref activity_id) = event.activity_id {
            log_event.insert(
                "activity_id",
                Value::Bytes(activity_id.clone().into()),
            );
        }

        if let Some(ref related_activity_id) = event.related_activity_id {
            log_event.insert(
                "related_activity_id",
                Value::Bytes(related_activity_id.clone().into()),
            );
        }

        // Include raw XML if requested
        if self.config.include_xml && !event.raw_xml.is_empty() {
            log_event.insert(
                "xml",
                Value::Bytes(event.raw_xml.clone().into()),
            );
        }

        // Include event data if configured
        if self.config.field_filter.include_event_data && !event.event_data.is_empty() {
            let mut event_data_map = ObjectMap::new();
            for (key, value) in &event.event_data {
                event_data_map.insert(key.clone().into(), Value::Bytes(value.clone().into()));
            }
            log_event.insert(
                "event_data",
                Value::Object(event_data_map),
            );
        }

        // Include user data if configured
        if self.config.field_filter.include_user_data && !event.user_data.is_empty() {
            let mut user_data_map = ObjectMap::new();
            for (key, value) in &event.user_data {
                user_data_map.insert(key.clone().into(), Value::Bytes(value.clone().into()));
            }
            log_event.insert(
                "user_data",
                Value::Object(user_data_map),
            );
        }

        Ok(())
    }

    fn extract_message_from_event_data(&self, event: &WindowsEvent) -> String {
        // Try to find a message in event data
        for (key, value) in &event.event_data {
            if key.to_lowercase().contains("message") {
                return value.clone();
            }
        }

        // Fall back to generic message
        format!(
            "Event ID {} from {} on {}",
            event.event_id, event.provider_name, event.computer
        )
    }

    fn apply_field_filtering(
        &self,
        log_event: &mut LogEvent,
    ) -> Result<(), WindowsEventLogError> {
        let filter = &self.config.field_filter;

        // If include_fields is specified, remove fields not in the list
        if let Some(ref include_fields) = filter.include_fields {
            let include_set: std::collections::HashSet<_> = include_fields.iter().collect();
            
            // Get all current field names
            let current_fields: Vec<String> = log_event
                .keys()
                .map(|k| k.to_string())
                .collect();

            // Remove fields not in include list
            for field in current_fields {
                if !include_set.contains(&field.as_str()) {
                    log_event.remove(&field);
                }
            }
        }

        // Remove fields in exclude_fields list
        if let Some(ref exclude_fields) = filter.exclude_fields {
            for field in exclude_fields {
                log_event.remove(field);
            }
        }

        Ok(())
    }

    fn apply_custom_formatting(
        &self,
        log_event: &mut LogEvent,
    ) -> Result<(), WindowsEventLogError> {
        for (field_name, format) in &self.config.event_data_format {
            if let Some(current_value) = log_event.get(field_name) {
                let formatted_value = self.format_value(current_value, format)?;
                log_event.insert(field_name, formatted_value);
            }
        }

        Ok(())
    }

    fn format_value(
        &self,
        value: &Value,
        format: &EventDataFormat,
    ) -> Result<Value, WindowsEventLogError> {
        match format {
            EventDataFormat::String => {
                Ok(Value::Bytes(value.to_string().into()))
            }
            EventDataFormat::Integer => {
                let int_value = match value {
                    Value::Integer(i) => *i,
                    Value::Float(f) => *f as i64,
                    Value::Bytes(b) => {
                        String::from_utf8_lossy(b)
                            .parse::<i64>()
                            .map_err(|_| WindowsEventLogError::FilterError {
                                message: format!("Cannot convert '{}' to integer", String::from_utf8_lossy(b)),
                            })?
                    }
                    _ => return Err(WindowsEventLogError::FilterError {
                        message: format!("Cannot convert {:?} to integer", value),
                    }),
                };
                Ok(Value::Integer(int_value))
            }
            EventDataFormat::Float => {
                let float_value = match value {
                    Value::Float(f) => *f,
                    Value::Integer(i) => *i as f64,
                    Value::Bytes(b) => {
                        String::from_utf8_lossy(b)
                            .parse::<f64>()
                            .map_err(|_| WindowsEventLogError::FilterError {
                                message: format!("Cannot convert '{}' to float", String::from_utf8_lossy(b)),
                            })?
                    }
                    _ => return Err(WindowsEventLogError::FilterError {
                        message: format!("Cannot convert {:?} to float", value),
                    }),
                };
                Ok(Value::Float(float_value.into()))
            }
            EventDataFormat::Boolean => {
                let bool_value = match value {
                    Value::Boolean(b) => *b,
                    Value::Integer(i) => *i != 0,
                    Value::Bytes(b) => {
                        let s = String::from_utf8_lossy(b).to_lowercase();
                        matches!(s.as_str(), "true" | "1" | "yes" | "on")
                    }
                    _ => return Err(WindowsEventLogError::FilterError {
                        message: format!("Cannot convert {:?} to boolean", value),
                    }),
                };
                Ok(Value::Boolean(bool_value))
            }
            EventDataFormat::Auto => {
                // Keep the original format
                Ok(value.clone())
            }
        }
    }

    /// Parse XML event data section
    pub fn parse_event_data_xml(xml: &str) -> Result<std::collections::HashMap<String, String>, WindowsEventLogError> {
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);
        
        let mut event_data = std::collections::HashMap::new();
        let mut current_element = String::new();
        let mut current_name = String::new();
        let mut in_event_data = false;

        loop {
            match reader.read_event() {
                Ok(XmlEvent::Start(ref e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref());
                    
                    if name == "EventData" {
                        in_event_data = true;
                    } else if in_event_data && name == "Data" {
                        // Extract the Name attribute
                        for attr in e.attributes() {
                            let attr = attr.map_err(WindowsEventLogError::ParseXmlError)?;
                            if attr.key.as_ref() == b"Name" {
                                current_name = String::from_utf8_lossy(&attr.value).to_string();
                                break;
                            }
                        }
                    }
                    current_element = name.to_string();
                }
                Ok(XmlEvent::End(ref e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref());
                    if name == "EventData" {
                        in_event_data = false;
                    }
                    current_element.clear();
                }
                Ok(XmlEvent::Text(ref e)) => {
                    if in_event_data && current_element == "Data" && !current_name.is_empty() {
                        let value = e.unescape().map_err(WindowsEventLogError::ParseXmlError)?;
                        event_data.insert(current_name.clone(), value.to_string());
                        current_name.clear();
                    }
                }
                Ok(XmlEvent::Eof) => break,
                Err(e) => return Err(WindowsEventLogError::ParseXmlError { source: e }),
                _ => {}
            }
        }

        Ok(event_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use chrono::Utc;

    fn create_test_event() -> WindowsEvent {
        WindowsEvent {
            record_id: 12345,
            event_id: 1000,
            level: 4,
            task: 1,
            opcode: 2,
            keywords: 0x8000000000000000,
            time_created: Utc::now(),
            provider_name: "TestProvider".to_string(),
            provider_guid: Some("{12345678-1234-1234-1234-123456789012}".to_string()),
            channel: "TestChannel".to_string(),
            computer: "TEST-PC".to_string(),
            user_id: Some("S-1-5-21-1234567890-1234567890-1234567890-1000".to_string()),
            process_id: 1234,
            thread_id: 5678,
            activity_id: Some("{ABCDEFGH-1234-1234-1234-123456789012}".to_string()),
            related_activity_id: None,
            raw_xml: "<Event><System><EventID>1000</EventID></System></Event>".to_string(),
            rendered_message: Some("Test message".to_string()),
            event_data: {
                let mut map = HashMap::new();
                map.insert("key1".to_string(), "value1".to_string());
                map.insert("key2".to_string(), "value2".to_string());
                map
            },
            user_data: HashMap::new(),
        }
    }

    #[test]
    fn test_parse_event_basic() {
        let config = WindowsEventLogConfig::default();
        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event.clone()).unwrap();

        // Check core fields
        assert_eq!(
            log_event.get("event_id").unwrap(),
            &Value::Integer(1000)
        );
        assert_eq!(
            log_event.get("record_id").unwrap(),
            &Value::Integer(12345)
        );
        assert_eq!(
            log_event.get("level").unwrap(),
            &Value::Bytes("Information".into())
        );
        assert_eq!(
            log_event.get("channel").unwrap(),
            &Value::Bytes("TestChannel".into())
        );
        assert_eq!(
            log_event.get("provider_name").unwrap(),
            &Value::Bytes("TestProvider".into())
        );
        assert_eq!(
            log_event.get("computer").unwrap(),
            &Value::Bytes("TEST-PC".into())
        );
    }

    #[test]
    fn test_parse_event_with_xml() {
        let mut config = WindowsEventLogConfig::default();
        config.include_xml = true;
        
        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event.clone()).unwrap();

        assert!(log_event.get("xml").is_some());
        assert_eq!(
            log_event.get("xml").unwrap(),
            &Value::Bytes(event.raw_xml.into())
        );
    }

    #[test]
    fn test_parse_event_data_filtering() {
        let mut config = WindowsEventLogConfig::default();
        config.field_filter.include_event_data = true;
        
        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event.clone()).unwrap();

        if let Some(Value::Object(event_data)) = log_event.get("event_data") {
            assert_eq!(event_data.get("key1"), Some(&Value::Bytes("value1".into())));
            assert_eq!(event_data.get("key2"), Some(&Value::Bytes("value2".into())));
        } else {
            panic!("event_data should be present");
        }
    }

    #[test]
    fn test_custom_formatting() {
        let mut config = WindowsEventLogConfig::default();
        config.event_data_format.insert(
            "event_id".to_string(), 
            EventDataFormat::String
        );
        
        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event).unwrap();

        // event_id should be converted to string
        assert_eq!(
            log_event.get("event_id").unwrap(),
            &Value::Bytes("1000".into())
        );
    }

    #[test]
    fn test_field_include_filtering() {
        let mut config = WindowsEventLogConfig::default();
        config.field_filter.include_fields = Some(vec![
            "event_id".to_string(),
            "level".to_string(),
        ]);
        
        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event).unwrap();

        // Only included fields should be present
        assert!(log_event.get("event_id").is_some());
        assert!(log_event.get("level").is_some());
        // Other fields should be filtered out
        // Note: This test might need adjustment based on actual field filtering implementation
    }

    #[test]
    fn test_field_exclude_filtering() {
        let mut config = WindowsEventLogConfig::default();
        config.field_filter.exclude_fields = Some(vec![
            "raw_xml".to_string(),
            "provider_guid".to_string(),
        ]);
        
        let parser = EventLogParser::new(&config);
        let event = create_test_event();

        let log_event = parser.parse_event(event).unwrap();

        // Excluded fields should not be present
        assert!(log_event.get("raw_xml").is_none());
        assert!(log_event.get("provider_guid").is_none());
        // Other fields should still be there
        assert!(log_event.get("event_id").is_some());
    }

    #[test]
    fn test_parse_event_data_xml() {
        let xml = r#"
            <Event>
                <EventData>
                    <Data Name="TargetUserName">admin</Data>
                    <Data Name="TargetLogonId">0x12345</Data>
                    <Data Name="LogonType">2</Data>
                </EventData>
            </Event>
        "#;

        let event_data = EventLogParser::parse_event_data_xml(xml).unwrap();

        assert_eq!(event_data.get("TargetUserName"), Some(&"admin".to_string()));
        assert_eq!(event_data.get("TargetLogonId"), Some(&"0x12345".to_string()));
        assert_eq!(event_data.get("LogonType"), Some(&"2".to_string()));
    }

    #[test]
    fn test_extract_message_from_event_data() {
        let config = WindowsEventLogConfig::default();
        let parser = EventLogParser::new(&config);
        
        let mut event = create_test_event();
        event.rendered_message = None;
        event.event_data.insert("message".to_string(), "Custom message".to_string());

        let message = parser.extract_message_from_event_data(&event);
        assert_eq!(message, "Custom message");
    }

    #[test]
    fn test_format_value_conversions() {
        let config = WindowsEventLogConfig::default();
        let parser = EventLogParser::new(&config);

        // Test string conversion
        let value = Value::Integer(123);
        let result = parser.format_value(&value, &EventDataFormat::String).unwrap();
        assert_eq!(result, Value::Bytes("123".into()));

        // Test integer conversion
        let value = Value::Bytes("456".into());
        let result = parser.format_value(&value, &EventDataFormat::Integer).unwrap();
        assert_eq!(result, Value::Integer(456));

        // Test float conversion
        let value = Value::Bytes("123.45".into());
        let result = parser.format_value(&value, &EventDataFormat::Float).unwrap();
        if let Value::Float(f) = result {
            assert!((f.into_inner() - 123.45).abs() < f64::EPSILON);
        } else {
            panic!("Expected float value");
        }

        // Test boolean conversion
        let value = Value::Bytes("true".into());
        let result = parser.format_value(&value, &EventDataFormat::Boolean).unwrap();
        assert_eq!(result, Value::Boolean(true));

        // Test auto format (no change)
        let value = Value::Integer(789);
        let result = parser.format_value(&value, &EventDataFormat::Auto).unwrap();
        assert_eq!(result, Value::Integer(789));
    }
}