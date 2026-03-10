use std::collections::HashMap;

use chrono::{DateTime, Utc};
use metrics::counter;
use quick_xml::{Reader, events::Event as XmlEvent};

use super::config::WindowsEventLogConfig;
use super::error::*;

/// Truncate a string at a UTF-8 safe boundary, appending a suffix.
pub(crate) fn truncate_utf8(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push_str("...[truncated]");
}

/// System fields extracted from Windows Event Log XML via single-pass parsing.
#[derive(Debug, Clone, Default)]
pub struct SystemFields {
    pub event_id: u32,
    pub level: u8,
    pub task: u16,
    pub opcode: u8,
    pub keywords: u64,
    pub version: Option<u8>,
    pub qualifiers: Option<u16>,
    pub record_id: u64,
    pub activity_id: Option<String>,
    pub related_activity_id: Option<String>,
    pub process_id: u32,
    pub thread_id: u32,
    pub channel: String,
    pub computer: String,
    pub user_id: Option<String>,
    pub provider_name: String,
    pub provider_guid: Option<String>,
    /// Raw timestamp string from TimeCreated/@SystemTime.
    pub system_time: Option<String>,
}

/// Result from EventData parsing (supports both named and positional formats).
#[derive(Debug, Clone)]
pub struct EventDataResult {
    pub structured_data: HashMap<String, String>,
    pub string_inserts: Vec<String>,
    pub user_data: HashMap<String, String>,
}

/// Represents a Windows Event Log event.
#[derive(Debug, Clone)]
pub struct WindowsEvent {
    pub record_id: u64,
    pub event_id: u32,
    pub level: u8,
    pub task: u16,
    pub opcode: u8,
    pub keywords: u64,
    pub time_created: DateTime<Utc>,
    pub provider_name: String,
    pub provider_guid: Option<String>,
    pub channel: String,
    pub computer: String,
    pub user_id: Option<String>,
    pub process_id: u32,
    pub thread_id: u32,
    pub activity_id: Option<String>,
    pub related_activity_id: Option<String>,
    pub raw_xml: String,
    pub rendered_message: Option<String>,
    pub event_data: HashMap<String, String>,
    pub user_data: HashMap<String, String>,
    pub task_name: Option<String>,
    pub opcode_name: Option<String>,
    pub keyword_names: Vec<String>,
    /// Resolved account name from user_id SID (e.g. "NT AUTHORITY\SYSTEM").
    pub user_name: Option<String>,
    pub version: Option<u8>,
    pub qualifiers: Option<u16>,
    pub string_inserts: Vec<String>,
}

impl WindowsEvent {
    /// Returns the human-readable level name for this event.
    ///
    /// Level 0 maps to "Information" per standard convention. Windows uses
    /// Level=0 for "LogAlways" and for all Security audit events. Mapping it to
    /// "Information" prevents SOC analysts from seeing "Unknown" on every logon event.
    pub const fn level_name(&self) -> &'static str {
        match self.level {
            0 => "Information",
            1 => "Critical",
            2 => "Error",
            3 => "Warning",
            4 => "Information",
            5 => "Verbose",
            _ => "Unknown",
        }
    }
}

/// Tracks which element's text content we are currently collecting.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TextTarget {
    None,
    EventID,
    Version,
    Level,
    Task,
    Opcode,
    Keywords,
    EventRecordID,
    Channel,
    Computer,
}

/// Parse the System section of Windows Event Log XML in a single pass.
///
/// Replaces ~28 individual `extract_xml_value`/`extract_xml_attribute`/
/// `extract_provider_name` calls with one `quick_xml::Reader` traversal.
pub fn parse_system_section(xml: &str) -> SystemFields {
    let mut fields = SystemFields::default();
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut buf = Vec::new();

    let mut in_system = false;
    let mut text_target = TextTarget::None;
    let mut text_buf = String::new();

    const MAX_ITERATIONS: usize = 2000;
    let mut iterations = 0;

    loop {
        if iterations >= MAX_ITERATIONS {
            break;
        }
        iterations += 1;

        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(ref e)) => {
                let local = e.name().local_name();
                let local = local.as_ref();

                if local == b"System" {
                    in_system = true;
                } else if in_system {
                    text_target = TextTarget::None;
                    text_buf.clear();

                    match local {
                        b"Provider" => extract_provider_attrs(e, &mut fields),
                        b"EventID" => {
                            extract_qualifiers_attr(e, &mut fields);
                            text_target = TextTarget::EventID;
                        }
                        b"Version" => text_target = TextTarget::Version,
                        b"Level" => text_target = TextTarget::Level,
                        b"Task" => text_target = TextTarget::Task,
                        b"Opcode" => text_target = TextTarget::Opcode,
                        b"Keywords" => text_target = TextTarget::Keywords,
                        b"TimeCreated" => extract_time_created_attr(e, &mut fields),
                        b"EventRecordID" => text_target = TextTarget::EventRecordID,
                        b"Correlation" => extract_correlation_attrs(e, &mut fields),
                        b"Execution" => extract_execution_attrs(e, &mut fields),
                        b"Channel" => text_target = TextTarget::Channel,
                        b"Computer" => text_target = TextTarget::Computer,
                        b"Security" => extract_security_attrs(e, &mut fields),
                        _ => {}
                    }
                }
            }
            Ok(XmlEvent::Empty(ref e)) => {
                if !in_system {
                    if e.name().local_name().as_ref() == b"System" {
                        // Empty <System/> — nothing to extract
                        break;
                    }
                    buf.clear();
                    continue;
                }
                let local = e.name().local_name();
                let local = local.as_ref();
                match local {
                    b"Provider" => extract_provider_attrs(e, &mut fields),
                    b"TimeCreated" => extract_time_created_attr(e, &mut fields),
                    b"Correlation" => extract_correlation_attrs(e, &mut fields),
                    b"Execution" => extract_execution_attrs(e, &mut fields),
                    b"Security" => extract_security_attrs(e, &mut fields),
                    _ => {}
                }
            }
            Ok(XmlEvent::Text(ref e)) => {
                if in_system && text_target != TextTarget::None {
                    if let Ok(text) = e.unescape() {
                        if text_buf.len() + text.len() <= 4096 {
                            text_buf.push_str(&text);
                        }
                    }
                }
            }
            Ok(XmlEvent::End(ref e)) => {
                let local = e.name().local_name();
                let local = local.as_ref();
                if local == b"System" {
                    // Commit any pending text before exiting
                    commit_text(&text_target, &text_buf, &mut fields);
                    break;
                }
                if in_system && text_target != TextTarget::None {
                    commit_text(&text_target, &text_buf, &mut fields);
                    text_target = TextTarget::None;
                    text_buf.clear();
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }

        buf.clear();
    }

    fields
}

/// Commit collected element text into the appropriate SystemFields field.
fn commit_text(target: &TextTarget, text: &str, fields: &mut SystemFields) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    match target {
        TextTarget::EventID => fields.event_id = trimmed.parse().unwrap_or(0),
        TextTarget::Version => fields.version = trimmed.parse().ok(),
        TextTarget::Level => fields.level = trimmed.parse().unwrap_or(0),
        TextTarget::Task => fields.task = trimmed.parse().unwrap_or(0),
        TextTarget::Opcode => fields.opcode = trimmed.parse().unwrap_or(0),
        TextTarget::Keywords => fields.keywords = parse_keywords_hex(trimmed),
        TextTarget::EventRecordID => fields.record_id = trimmed.parse().unwrap_or(0),
        TextTarget::Channel => fields.channel = trimmed.to_string(),
        TextTarget::Computer => fields.computer = trimmed.to_string(),
        TextTarget::None => {}
    }
}

fn parse_keywords_hex(s: &str) -> u64 {
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .or_else(|| s.parse::<u64>().ok())
        .unwrap_or(0)
}

fn extract_provider_attrs(e: &quick_xml::events::BytesStart<'_>, fields: &mut SystemFields) {
    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"Name" => fields.provider_name = String::from_utf8_lossy(&attr.value).into_owned(),
            b"Guid" => {
                fields.provider_guid = Some(String::from_utf8_lossy(&attr.value).into_owned())
            }
            _ => {}
        }
    }
}

fn extract_qualifiers_attr(e: &quick_xml::events::BytesStart<'_>, fields: &mut SystemFields) {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"Qualifiers" {
            fields.qualifiers = String::from_utf8_lossy(&attr.value).parse().ok();
        }
    }
}

fn extract_time_created_attr(e: &quick_xml::events::BytesStart<'_>, fields: &mut SystemFields) {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"SystemTime" {
            fields.system_time = Some(String::from_utf8_lossy(&attr.value).into_owned());
        }
    }
}

fn extract_correlation_attrs(e: &quick_xml::events::BytesStart<'_>, fields: &mut SystemFields) {
    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"ActivityID" => {
                fields.activity_id = Some(String::from_utf8_lossy(&attr.value).into_owned())
            }
            b"RelatedActivityID" => {
                fields.related_activity_id = Some(String::from_utf8_lossy(&attr.value).into_owned())
            }
            _ => {}
        }
    }
}

fn extract_execution_attrs(e: &quick_xml::events::BytesStart<'_>, fields: &mut SystemFields) {
    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"ProcessID" => {
                fields.process_id = String::from_utf8_lossy(&attr.value).parse().unwrap_or(0)
            }
            b"ThreadID" => {
                fields.thread_id = String::from_utf8_lossy(&attr.value).parse().unwrap_or(0)
            }
            _ => {}
        }
    }
}

fn extract_security_attrs(e: &quick_xml::events::BytesStart<'_>, fields: &mut SystemFields) {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"UserID" {
            fields.user_id = Some(String::from_utf8_lossy(&attr.value).into_owned());
        }
    }
}

/// Build a WindowsEvent from pre-parsed SystemFields and raw XML.
///
/// Applies event ID filters, age filters, and parses EventData/UserData.
/// Returns `Ok(None)` for filtered events.
pub fn build_event(
    xml: String,
    channel: &str,
    config: &WindowsEventLogConfig,
    rendered_message: Option<String>,
    system_fields: SystemFields,
) -> Result<Option<WindowsEvent>, WindowsEventLogError> {
    let record_id = system_fields.record_id;
    let event_id = system_fields.event_id;

    if record_id == 0 && event_id == 0 {
        debug!(
            message = "Failed to parse event XML - no valid EventID or RecordID found.",
            channel = %channel
        );
        return Ok(None);
    }

    // Apply event ID filters early
    if let Some(ref only_ids) = config.only_event_ids
        && !only_ids.contains(&event_id)
    {
        counter!("windows_event_log_events_filtered_total", "reason" => "event_id_not_in_only_list")
            .increment(1);
        return Ok(None);
    }

    if config.ignore_event_ids.contains(&event_id) {
        counter!("windows_event_log_events_filtered_total", "reason" => "event_id_ignored")
            .increment(1);
        return Ok(None);
    }

    // Parse timestamp
    let time_created = system_fields
        .system_time
        .as_deref()
        .and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%z"))
                .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%z"))
                .ok()
        })
        .map(|dt| {
            let dt_utc = dt.with_timezone(&Utc);
            let diff = (Utc::now() - dt_utc).num_days().abs();
            if diff > 365 * 10 {
                warn!(
                    message = "Event timestamp is more than 10 years from current time.",
                    timestamp = %dt_utc,
                    channel = %channel,
                    record_id = record_id,
                );
            }
            dt_utc
        })
        .unwrap_or_else(Utc::now);

    // Apply age filter
    if let Some(max_age_secs) = config.max_event_age_secs {
        let age = Utc::now().signed_duration_since(time_created);
        if age.num_seconds() > max_age_secs as i64 {
            counter!("windows_event_log_events_filtered_total", "reason" => "max_age_exceeded")
                .increment(1);
            return Ok(None);
        }
    }

    let event_data_result = extract_event_data(&xml, config);

    let event = WindowsEvent {
        record_id,
        event_id,
        level: system_fields.level,
        task: system_fields.task,
        opcode: system_fields.opcode,
        keywords: system_fields.keywords,
        time_created,
        provider_name: system_fields.provider_name,
        provider_guid: system_fields.provider_guid,
        channel: if system_fields.channel.is_empty() {
            channel.to_string()
        } else {
            system_fields.channel
        },
        computer: system_fields.computer,
        user_id: system_fields.user_id,
        process_id: system_fields.process_id,
        thread_id: system_fields.thread_id,
        activity_id: system_fields.activity_id,
        related_activity_id: system_fields.related_activity_id,
        rendered_message,
        raw_xml: if config.include_xml {
            let mut raw = xml;
            truncate_utf8(&mut raw, 32768);
            raw
        } else {
            String::new()
        },
        event_data: event_data_result.structured_data,
        user_data: event_data_result.user_data,
        task_name: None,
        opcode_name: None,
        keyword_names: Vec::new(),
        user_name: None,
        version: system_fields.version,
        qualifiers: system_fields.qualifiers,
        string_inserts: event_data_result.string_inserts,
    };

    Ok(Some(event))
}

/// Convenience wrapper: parse System section + build event in one call.
#[cfg(test)]
pub fn parse_event_xml(
    xml: String,
    channel: &str,
    config: &WindowsEventLogConfig,
    rendered_message: Option<String>,
) -> Result<Option<WindowsEvent>, WindowsEventLogError> {
    let system_fields = parse_system_section(&xml);
    build_event(xml, channel, config, rendered_message, system_fields)
}

/// Extract EventData and UserData sections from event XML.
pub fn extract_event_data(xml: &str, config: &WindowsEventLogConfig) -> EventDataResult {
    let mut structured_data = HashMap::new();
    let mut string_inserts = Vec::new();
    let mut user_data = HashMap::new();

    parse_section(xml, "EventData", &mut structured_data, &mut string_inserts);
    parse_section(xml, "UserData", &mut user_data, &mut Vec::new());

    // Apply configurable truncation
    if config.max_event_data_length > 0 {
        for value in structured_data.values_mut() {
            truncate_utf8(value, config.max_event_data_length);
        }
        for value in user_data.values_mut() {
            truncate_utf8(value, config.max_event_data_length);
        }
        for value in string_inserts.iter_mut() {
            truncate_utf8(value, config.max_event_data_length);
        }
    }

    EventDataResult {
        structured_data,
        string_inserts,
        user_data,
    }
}

/// Parse a specific XML section (EventData or UserData).
fn parse_section(
    xml: &str,
    section_name: &str,
    named_data: &mut HashMap<String, String>,
    inserts: &mut Vec<String>,
) {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut inside_section = false;
    let mut inside_data = false;
    let mut current_data_name = String::new();
    let mut current_data_value = String::new();

    const MAX_ITERATIONS: usize = 500;
    const MAX_FIELDS: usize = 100;
    let mut iterations = 0;

    loop {
        if iterations >= MAX_ITERATIONS
            || (named_data.len() >= MAX_FIELDS || inserts.len() >= MAX_FIELDS)
        {
            break;
        }
        iterations += 1;

        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(ref e)) => {
                let name = e.name();
                if name.as_ref() == section_name.as_bytes() {
                    inside_section = true;
                } else if inside_section && name.as_ref() == b"Data" {
                    inside_data = true;
                    current_data_name.clear();
                    current_data_value.clear();

                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"Name" {
                            let name_value = String::from_utf8_lossy(&attr.value);
                            if name_value.len() <= 128 && !name_value.trim().is_empty() {
                                current_data_name = name_value.into_owned();
                            }
                            break;
                        }
                    }
                }
            }
            Ok(XmlEvent::End(ref e)) => {
                let name = e.name();
                if name.as_ref() == section_name.as_bytes() {
                    inside_section = false;
                } else if name.as_ref() == b"Data" && inside_data {
                    inside_data = false;

                    if !current_data_name.is_empty() {
                        named_data.insert(current_data_name.clone(), current_data_value.clone());
                    } else if section_name == "EventData" && inserts.len() < MAX_FIELDS {
                        inserts.push(current_data_value.clone());
                    }
                }
            }
            Ok(XmlEvent::Text(ref e)) => {
                if inside_section
                    && inside_data
                    && let Ok(text) = e.unescape()
                {
                    const MAX_VALUE_SIZE: usize = 1024 * 1024;
                    if current_data_value.len() + text.len() <= MAX_VALUE_SIZE {
                        current_data_value.push_str(&text);
                    }
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => break,
            _ => {}
        }

        buf.clear();
    }
}

/// Check if bookmark XML is valid (contains an actual bookmark position).
pub fn is_valid_bookmark_xml(xml: &str) -> bool {
    !xml.is_empty() && xml.contains("<Bookmark") && xml.contains("RecordId")
}

/// Extract a single element's text content by tag name.
///
/// Prefer `parse_system_section` for bulk System field extraction (single pass).
/// This function is retained for one-off lookups outside the hot path.
#[cfg(test)]
pub fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut inside_target = false;
    let mut current_element = String::new();

    const MAX_ITERATIONS: usize = 5000;
    let mut iterations = 0;

    loop {
        if iterations >= MAX_ITERATIONS {
            warn!(message = "XML parsing iteration limit exceeded.");
            return None;
        }
        iterations += 1;

        match reader.read_event_into(&mut buf) {
            Ok(XmlEvent::Start(ref e)) => {
                let name = e.name();
                let element_name = String::from_utf8_lossy(name.as_ref());
                if element_name == tag {
                    inside_target = true;
                    current_element.clear();
                }
            }
            Ok(XmlEvent::Text(ref e)) => {
                if inside_target {
                    match e.unescape() {
                        Ok(text) => {
                            if current_element.len() + text.len() > 4096 {
                                warn!(message = "XML element text too long, truncating.");
                                break;
                            }
                            current_element.push_str(&text);
                        }
                        Err(_) => return None,
                    }
                }
            }
            Ok(XmlEvent::End(ref e)) => {
                let name = e.name();
                let element_name = String::from_utf8_lossy(name.as_ref());
                if element_name == tag && inside_target {
                    return Some(current_element.trim().to_string());
                }
            }
            Ok(XmlEvent::Eof) => break,
            Err(_) => return None,
            _ => {}
        }

        buf.clear();
    }

    None
}

/// Extract an XML attribute value by attribute name via string search.
///
/// Prefer `parse_system_section` for bulk System field extraction (single pass).
/// This function is retained for one-off lookups outside the hot path.
#[cfg(test)]
pub fn extract_xml_attribute(xml: &str, attr_name: &str) -> Option<String> {
    let needle = format!("{attr_name}='");
    if let Some(start) = xml.find(&needle) {
        let value_start = start + needle.len();
        if let Some(end) = xml[value_start..].find('\'') {
            return Some(xml[value_start..value_start + end].to_string());
        }
    }
    let needle = format!("{attr_name}=\"");
    if let Some(start) = xml.find(&needle) {
        let value_start = start + needle.len();
        if let Some(end) = xml[value_start..].find('"') {
            return Some(xml[value_start..value_start + end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_EVENT_XML: &str = r#"
    <Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
        <System>
            <Provider Name="Microsoft-Windows-Kernel-General" Guid="{A68CA8B7-004F-D7B6-A698-07E2DE0F1F5D}"/>
            <EventID Qualifiers="16384">1</EventID>
            <Version>2</Version>
            <Level>4</Level>
            <Task>100</Task>
            <Opcode>1</Opcode>
            <Keywords>0x8000000000000000</Keywords>
            <TimeCreated SystemTime="2025-08-29T00:15:41.123456Z"/>
            <EventRecordID>12345</EventRecordID>
            <Correlation ActivityID="{AAAA-BBBB}" RelatedActivityID="{CCCC-DDDD}"/>
            <Execution ProcessID="1234" ThreadID="5678"/>
            <Channel>System</Channel>
            <Computer>TEST-MACHINE</Computer>
            <Security UserID="S-1-5-18"/>
        </System>
    </Event>
    "#;

    #[test]
    fn test_parse_system_section_full() {
        let fields = parse_system_section(FULL_EVENT_XML);

        assert_eq!(fields.provider_name, "Microsoft-Windows-Kernel-General");
        assert_eq!(
            fields.provider_guid.as_deref(),
            Some("{A68CA8B7-004F-D7B6-A698-07E2DE0F1F5D}")
        );
        assert_eq!(fields.event_id, 1);
        assert_eq!(fields.qualifiers, Some(16384));
        assert_eq!(fields.version, Some(2));
        assert_eq!(fields.level, 4);
        assert_eq!(fields.task, 100);
        assert_eq!(fields.opcode, 1);
        assert_eq!(fields.keywords, 0x8000000000000000);
        assert_eq!(
            fields.system_time.as_deref(),
            Some("2025-08-29T00:15:41.123456Z")
        );
        assert_eq!(fields.record_id, 12345);
        assert_eq!(fields.activity_id.as_deref(), Some("{AAAA-BBBB}"));
        assert_eq!(fields.related_activity_id.as_deref(), Some("{CCCC-DDDD}"));
        assert_eq!(fields.process_id, 1234);
        assert_eq!(fields.thread_id, 5678);
        assert_eq!(fields.channel, "System");
        assert_eq!(fields.computer, "TEST-MACHINE");
        assert_eq!(fields.user_id.as_deref(), Some("S-1-5-18"));
    }

    #[test]
    fn test_parse_system_section_minimal() {
        let xml = r#"
        <Event>
            <System>
                <Provider Name="TestProvider"/>
                <EventID>42</EventID>
                <Channel>Application</Channel>
                <Computer>PC</Computer>
            </System>
        </Event>
        "#;

        let fields = parse_system_section(xml);
        assert_eq!(fields.provider_name, "TestProvider");
        assert_eq!(fields.event_id, 42);
        assert_eq!(fields.channel, "Application");
        assert_eq!(fields.computer, "PC");
        assert_eq!(fields.level, 0);
        assert_eq!(fields.record_id, 0);
        assert!(fields.provider_guid.is_none());
        assert!(fields.system_time.is_none());
    }

    #[test]
    fn test_parse_system_section_stops_at_end_of_system() {
        // Ensure parser stops after </System> and doesn't scan EventData
        let xml = r#"
        <Event>
            <System>
                <Provider Name="P1"/>
                <EventID>1</EventID>
                <Channel>App</Channel>
                <Computer>PC</Computer>
            </System>
            <EventData>
                <Data Name="Channel">ShouldNotBeUsed</Data>
            </EventData>
        </Event>
        "#;

        let fields = parse_system_section(xml);
        assert_eq!(fields.channel, "App");
        assert_eq!(fields.provider_name, "P1");
    }

    #[test]
    fn test_extract_xml_value() {
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
    fn test_extract_xml_attribute() {
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
    fn test_windows_event_level_name() {
        let event = WindowsEvent {
            record_id: 1,
            event_id: 1000,
            level: 2,
            task: 0,
            opcode: 0,
            keywords: 0,
            time_created: Utc::now(),
            provider_name: "Test".to_string(),
            provider_guid: None,
            channel: "Test".to_string(),
            computer: "localhost".to_string(),
            user_id: None,
            process_id: 0,
            thread_id: 0,
            activity_id: None,
            related_activity_id: None,
            raw_xml: String::new(),
            rendered_message: None,
            event_data: HashMap::new(),
            user_data: HashMap::new(),
            task_name: None,
            opcode_name: None,
            keyword_names: Vec::new(),
            user_name: None,
            version: Some(1),
            qualifiers: Some(0),
            string_inserts: vec![],
        };

        assert_eq!(event.level_name(), "Error");
    }

    #[test]
    fn test_level_0_maps_to_information() {
        let mut event = WindowsEvent {
            record_id: 1,
            event_id: 4624,
            level: 0,
            task: 12544,
            opcode: 0,
            keywords: 0x0020000000000000,
            time_created: Utc::now(),
            provider_name: "Microsoft-Windows-Security-Auditing".to_string(),
            provider_guid: None,
            channel: "Security".to_string(),
            computer: "localhost".to_string(),
            user_id: None,
            process_id: 0,
            thread_id: 0,
            activity_id: None,
            related_activity_id: None,
            raw_xml: String::new(),
            rendered_message: None,
            event_data: HashMap::new(),
            user_data: HashMap::new(),
            task_name: None,
            opcode_name: None,
            keyword_names: Vec::new(),
            user_name: None,
            version: Some(2),
            qualifiers: Some(0),
            string_inserts: vec![],
        };

        assert_eq!(event.level_name(), "Information");

        event.level = 4;
        assert_eq!(event.level_name(), "Information");
    }

    #[test]
    fn test_security_limits() {
        let large_xml = format!(
            r#"
        <Event>
            <System>
                <EventID>{}</EventID>
            </System>
        </Event>
        "#,
            "x".repeat(10000)
        );

        let result = extract_xml_value(&large_xml, "EventID");
        assert!(
            result.is_none(),
            "Security limits should reject excessively large XML content"
        );
    }

    #[test]
    fn test_configurable_truncation_disabled_by_default() {
        let config = WindowsEventLogConfig::default();
        assert_eq!(
            config.max_event_data_length, 0,
            "Event data truncation should be disabled by default"
        );
    }

    #[test]
    fn test_event_data_truncation_when_enabled() {
        let xml = r#"
        <Event>
            <EventData>
                <Data Name="LongValue">This is a very long value that should be truncated when the limit is set</Data>
                <Data Name="ShortValue">Short</Data>
            </EventData>
        </Event>
        "#;

        let mut config = WindowsEventLogConfig::default();
        config.max_event_data_length = 20;

        let result = extract_event_data(xml, &config);

        let long_value = result.structured_data.get("LongValue").unwrap();
        assert!(
            long_value.ends_with("...[truncated]"),
            "Long value should be truncated"
        );
        assert!(
            long_value.len() <= 20 + "...[truncated]".len(),
            "Truncated value should respect limit"
        );

        let short_value = result.structured_data.get("ShortValue").unwrap();
        assert_eq!(short_value, "Short", "Short value should not be truncated");
        assert!(
            !short_value.contains("truncated"),
            "Short value should not have truncation marker"
        );
    }

    #[test]
    fn test_event_data_no_truncation_when_disabled() {
        let xml = r#"
        <Event>
            <EventData>
                <Data Name="LongValue">This is a very long value that should NOT be truncated when truncation is disabled by setting max_event_data_length to 0</Data>
            </EventData>
        </Event>
        "#;

        let config = WindowsEventLogConfig::default();
        assert_eq!(
            config.max_event_data_length, 0,
            "Default should be no truncation"
        );

        let result = extract_event_data(xml, &config);

        let long_value = result.structured_data.get("LongValue").unwrap();
        assert!(
            !long_value.ends_with("...[truncated]"),
            "Value should not be truncated when limit is 0"
        );
        assert!(long_value.len() > 100, "Full value should be preserved");
        assert!(
            long_value.contains("disabled by setting max_event_data_length to 0"),
            "Full text should be present"
        );
    }

    #[test]
    fn test_is_valid_bookmark_xml() {
        let valid = r#"<BookmarkList>
  <Bookmark Channel='Application' RecordId='12345' IsCurrent='true'/>
</BookmarkList>"#;
        assert!(
            is_valid_bookmark_xml(valid),
            "Should accept valid bookmark with RecordId"
        );

        assert!(!is_valid_bookmark_xml(""), "Should reject empty string");

        let empty_list = "<BookmarkList/>";
        assert!(
            !is_valid_bookmark_xml(empty_list),
            "Should reject empty BookmarkList"
        );

        let empty_list2 = "<BookmarkList></BookmarkList>";
        assert!(
            !is_valid_bookmark_xml(empty_list2),
            "Should reject BookmarkList without Bookmark element"
        );

        let no_record_id = "<BookmarkList><Bookmark Channel='System'/></BookmarkList>";
        assert!(
            !is_valid_bookmark_xml(no_record_id),
            "Should reject Bookmark without RecordId"
        );
    }

    #[test]
    fn test_parse_event_xml_basic() {
        let xml = r#"
        <Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
            <System>
                <Provider Name="TestProvider"/>
                <EventID>1000</EventID>
                <Level>4</Level>
                <EventRecordID>12345</EventRecordID>
                <TimeCreated SystemTime="2025-01-01T00:00:00.000000Z"/>
                <Channel>Application</Channel>
                <Computer>TEST-PC</Computer>
            </System>
        </Event>
        "#;

        let config = WindowsEventLogConfig::default();

        let result = parse_event_xml(xml.to_string(), "Application", &config, None);

        let event = result.unwrap().unwrap();
        assert_eq!(event.event_id, 1000);
        assert_eq!(event.record_id, 12345);
        assert_eq!(event.provider_name, "TestProvider");
        assert_eq!(event.channel, "Application");
        assert_eq!(event.computer, "TEST-PC");
        assert!(
            event.rendered_message.is_none(),
            "rendered_message should be None when not provided"
        );
    }

    #[test]
    fn test_parse_event_xml_with_rendered_message() {
        let xml = r#"
        <Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
            <System>
                <Provider Name="TestProvider"/>
                <EventID>1000</EventID>
                <Level>4</Level>
                <EventRecordID>12345</EventRecordID>
                <TimeCreated SystemTime="2025-01-01T00:00:00.000000Z"/>
                <Channel>Application</Channel>
                <Computer>TEST-PC</Computer>
            </System>
        </Event>
        "#;

        let config = WindowsEventLogConfig::default();
        let rendered_msg = Some("The application started successfully.".to_string());

        let result = parse_event_xml(xml.to_string(), "Application", &config, rendered_msg);

        let event = result.unwrap().unwrap();
        assert_eq!(event.event_id, 1000);
        assert_eq!(
            event.rendered_message,
            Some("The application started successfully.".to_string())
        );
    }

    #[test]
    fn test_keywords_hex_parsing() {
        assert_eq!(parse_keywords_hex("0x8000000000000000"), 0x8000000000000000);
        assert_eq!(parse_keywords_hex("0X8000000000000000"), 0x8000000000000000);
        assert_eq!(parse_keywords_hex("12345"), 12345);
        assert_eq!(parse_keywords_hex("invalid"), 0);
        assert_eq!(parse_keywords_hex("0x0020000000000000"), 0x0020000000000000);
    }

    #[test]
    fn test_max_event_age_secs_filters_old_events() {
        let xml = r#"
        <Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
            <System>
                <Provider Name="TestProvider"/>
                <EventID>1000</EventID>
                <Level>4</Level>
                <EventRecordID>12345</EventRecordID>
                <TimeCreated SystemTime="2020-01-01T00:00:00.000000Z"/>
                <Channel>Application</Channel>
                <Computer>TEST-PC</Computer>
            </System>
        </Event>
        "#;

        let mut config = WindowsEventLogConfig::default();
        config.max_event_age_secs = Some(3600); // 1 hour

        let result = parse_event_xml(xml.to_string(), "Application", &config, None);
        assert!(
            result.unwrap().is_none(),
            "Old event should be filtered by max_event_age_secs"
        );
    }

    #[test]
    fn test_max_event_age_secs_allows_recent_events() {
        // Use a timestamp very close to now
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string();
        let xml = format!(
            r#"
            <Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
                <System>
                    <Provider Name="TestProvider"/>
                    <EventID>1000</EventID>
                    <Level>4</Level>
                    <EventRecordID>12345</EventRecordID>
                    <TimeCreated SystemTime="{now}"/>
                    <Channel>Application</Channel>
                    <Computer>TEST-PC</Computer>
                </System>
            </Event>
            "#
        );

        let mut config = WindowsEventLogConfig::default();
        config.max_event_age_secs = Some(3600); // 1 hour

        let result = parse_event_xml(xml, "Application", &config, None);
        assert!(
            result.unwrap().is_some(),
            "Recent event should pass max_event_age_secs filter"
        );
    }
}
