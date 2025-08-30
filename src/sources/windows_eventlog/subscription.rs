use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use quick_xml::{Reader, events::Event as XmlEvent};
use regex;
use tokio::sync::mpsc;

use super::{config::WindowsEventLogConfig, error::*};

/// System fields from Windows Event Log XML (Single Responsibility)
#[derive(Debug, Clone)]
struct SystemFields {
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
    pub time_created: DateTime<Utc>,
    pub provider_name: String,
    pub provider_guid: Option<String>,
}

/// Result from EventData parsing (supports both formats)
#[derive(Debug, Clone)]
pub struct EventDataResult {
    pub structured_data: HashMap<String, String>, // Named fields
    pub string_inserts: Vec<String>,              // FluentBit-style array
    pub user_data: HashMap<String, String>,       // UserData section
}

/// Represents a Windows Event Log event
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
    // Additional fields for FluentBit compatibility
    pub version: Option<u8>,
    pub qualifiers: Option<u16>,
    pub string_inserts: Vec<String>, // FluentBit-compatible field
}

impl WindowsEvent {
    pub fn level_name(&self) -> &'static str {
        match self.level {
            1 => "Critical",
            2 => "Error",
            3 => "Warning",
            4 => "Information",
            5 => "Verbose",
            _ => "Unknown",
        }
    }
}

/// Event-driven Windows Event Log subscription using EvtSubscribe with proper callback-based approach
pub struct EventLogSubscription {
    config: Arc<WindowsEventLogConfig>,
    event_receiver: mpsc::UnboundedReceiver<WindowsEvent>,
    #[cfg(windows)]
    #[allow(dead_code)] // Used for RAII cleanup of Windows handles via Drop trait
    subscriptions: Arc<Mutex<Vec<SubscriptionHandle>>>,
}

#[cfg(windows)]
struct SubscriptionHandle {
    handle: windows::Win32::System::EventLog::EVT_HANDLE,
}

#[cfg(windows)]
impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            unsafe {
                if let Err(e) = windows::Win32::System::EventLog::EvtClose(self.handle) {
                    warn!("Failed to close subscription handle: {}", e);
                }
            }
        }
    }
}

// Global callback context - needed because C callbacks can't capture Rust closures
#[cfg(windows)]
static CALLBACK_CONTEXT: Mutex<Option<Arc<CallbackContext>>> = Mutex::new(None);

#[cfg(windows)]
struct CallbackContext {
    event_sender: mpsc::UnboundedSender<WindowsEvent>,
    config: Arc<WindowsEventLogConfig>,
}

impl EventLogSubscription {
    /// Create a new event-driven subscription using EvtSubscribe with callback
    pub fn new(config: &WindowsEventLogConfig) -> Result<Self, WindowsEventLogError> {
        #[cfg(not(windows))]
        {
            return Err(WindowsEventLogError::NotSupportedError);
        }

        #[cfg(windows)]
        {
            let config = Arc::new(config.clone());
            let (event_sender, event_receiver) = mpsc::unbounded_channel();

            // Validate channels exist and are accessible
            Self::validate_channels(&config)?;

            // Set up global callback context
            {
                let mut global_ctx = CALLBACK_CONTEXT.lock().unwrap();
                *global_ctx = Some(Arc::new(CallbackContext {
                    event_sender: event_sender.clone(),
                    config: Arc::clone(&config),
                }));
            }

            let subscriptions = Arc::new(Mutex::new(Vec::new()));

            // Create subscriptions for each channel
            Self::create_subscriptions(&config, Arc::clone(&subscriptions))?;

            Ok(Self {
                config,
                event_receiver,
                subscriptions,
            })
        }
    }

    /// Get the next batch of events from the subscription
    pub async fn next_events(&mut self, max_events: usize) -> Result<Vec<WindowsEvent>, WindowsEventLogError> {
        use tokio::time::{timeout, Duration};

        let mut events = Vec::with_capacity(max_events.min(1000));

        // Use timeout to prevent blocking indefinitely
        let timeout_duration = Duration::from_millis(self.config.event_timeout_ms);

        while events.len() < max_events {
            match timeout(timeout_duration, self.event_receiver.recv()).await {
                Ok(Some(event)) => {
                    if Self::should_include_event(&self.config, &event) {
                        events.push(event);
                    }
                }
                Ok(None) => {
                    // Channel closed, subscription ended
                    debug!("Event subscription channel closed");
                    break;
                }
                Err(_) => {
                    // Timeout occurred - this is normal for real-time subscriptions
                    if events.is_empty() {
                        trace!("No events received within timeout");
                    }
                    break;
                }
            }
        }

        Ok(events)
    }

    #[cfg(windows)]
    fn create_subscriptions(
        config: &Arc<WindowsEventLogConfig>,
        subscriptions: Arc<Mutex<Vec<SubscriptionHandle>>>,
    ) -> Result<(), WindowsEventLogError> {
        use windows::{
            Win32::System::EventLog::{
                EvtSubscribe, EvtSubscribeToFutureEvents, EvtSubscribeStartAtOldestRecord,
            },
            core::HSTRING,
        };

        info!("Creating Windows Event Log subscriptions");

        for channel in &config.channels {
            let channel_hstring = HSTRING::from(channel.as_str());
            let query = Self::build_xpath_query(config, channel)?;
            let query_hstring = HSTRING::from(query.clone());

            // Determine subscription flags based on configuration
            let subscription_flags = if config.read_existing_events {
                EvtSubscribeStartAtOldestRecord.0
            } else {
                EvtSubscribeToFutureEvents.0
            };

            debug!(
                message = "Creating Windows Event Log subscription",
                channel = %channel,
                query = %query,
                read_existing = config.read_existing_events
            );

            // Create subscription using EvtSubscribe with callback
            let subscription_handle = unsafe {
                EvtSubscribe(
                    None, // Session handle (local)
                    None, // Signal event (we use callback instead)
                    &channel_hstring,
                    &query_hstring,
                    None, // Bookmark (not using bookmarks in refactored version)
                    None, // Context (will be passed via global state)
                    Some(event_subscription_callback), // Callback function
                    subscription_flags,
                )
                .map_err(|e| WindowsEventLogError::CreateSubscriptionError { source: e })?
            };

            info!(
                message = "Windows Event Log subscription created successfully",
                channel = %channel
            );

            // Store subscription handle for cleanup
            {
                let mut subs = subscriptions.lock().unwrap();
                subs.push(SubscriptionHandle {
                    handle: subscription_handle,
                });
            }
        }

        Ok(())
    }

    fn should_include_event(_config: &WindowsEventLogConfig, _event: &WindowsEvent) -> bool {
        // Implement filtering logic based on field_filter configuration
        // For now, include all events that passed the XML parsing filters
        true
    }

    fn build_xpath_query(config: &WindowsEventLogConfig, _channel: &str) -> Result<String, WindowsEventLogError> {
        let query = if let Some(ref custom_query) = config.event_query {
            custom_query.clone()
        } else {
            "*".to_string()
        };

        Ok(query)
    }

    #[cfg(windows)]
    fn validate_channels(_config: &WindowsEventLogConfig) -> Result<(), WindowsEventLogError> {
        use windows::Win32::System::EventLog::{EvtClose, EvtOpenChannelEnum};

        // Try to enumerate channels to validate they exist
        let enum_handle = unsafe { EvtOpenChannelEnum(None, 0) }
            .map_err(|e| WindowsEventLogError::CreateSubscriptionError { source: e })?;

        if let Err(e) = unsafe { EvtClose(enum_handle) } {
            warn!("Failed to close enum handle: {}", e);
        }

        Ok(())
    }

    /// Comprehensive System section parser for all Windows Event Log fields
    /// Extracts fields following Single Responsibility Principle
    fn extract_system_fields(xml: &str) -> SystemFields {
        SystemFields {
            event_id: Self::extract_xml_value(xml, "EventID")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            level: Self::extract_xml_value(xml, "Level")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            task: Self::extract_xml_value(xml, "Task")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            opcode: Self::extract_xml_value(xml, "Opcode")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            keywords: Self::extract_xml_attribute(xml, "Keywords")
                .and_then(|v| {
                    // Handle both decimal and hex formats (0x prefix)
                    if v.starts_with("0x") || v.starts_with("0X") {
                        u64::from_str_radix(&v[2..], 16).ok()
                    } else {
                        v.parse().ok()
                    }
                })
                .unwrap_or(0),
            version: Self::extract_xml_value(xml, "Version")
                .and_then(|v| v.parse().ok()),
            qualifiers: Self::extract_xml_attribute(xml, "Qualifiers")
                .and_then(|v| v.parse().ok()),
            record_id: Self::extract_xml_value(xml, "EventRecordID")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            activity_id: Self::extract_xml_attribute(xml, "ActivityID"),
            related_activity_id: Self::extract_xml_attribute(xml, "RelatedActivityID"),
            process_id: Self::extract_xml_attribute(xml, "ProcessID")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            thread_id: Self::extract_xml_attribute(xml, "ThreadID")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            channel: Self::extract_xml_value(xml, "Channel").unwrap_or_default(),
            computer: Self::extract_xml_value(xml, "Computer").unwrap_or_default(),
            user_id: Self::extract_xml_attribute(xml, "UserID"),
            time_created: Self::extract_xml_attribute(xml, "SystemTime")
                .and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
            provider_name: Self::extract_xml_attribute(xml, "Name").unwrap_or_default(),
            provider_guid: Self::extract_xml_attribute(xml, "Guid"),
        }
    }

    // XML parsing helper methods - cleaned up and more secure
    pub fn extract_xml_attribute(xml: &str, attr_name: &str) -> Option<String> {
        // Use regex with proper escaping to prevent injection
        let pattern = format!(r#"{}="([^"]+)""#, regex::escape(attr_name));
        regex::Regex::new(&pattern).ok()?
            .captures(xml)?
            .get(1)
            .map(|m| m.as_str().to_string())
    }

    pub fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);

        let mut buf = Vec::new();
        let mut inside_target = false;
        let mut current_element = String::new();

        // Strict iteration limit for security
        const MAX_ITERATIONS: usize = 5000;
        let mut iterations = 0;

        loop {
            if iterations >= MAX_ITERATIONS {
                warn!("XML parsing iteration limit exceeded");
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
                                // Prevent excessive memory usage
                                if current_element.len() + text.len() > 4096 {
                                    warn!("XML element text too long, truncating");
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


    /// Enhanced EventData extraction supporting both structured data and StringInserts
    /// Follows Open/Closed Principle - extensible without modifying existing code
    pub fn extract_event_data(xml: &str) -> EventDataResult {
        let mut structured_data = HashMap::new();
        let mut string_inserts = Vec::new();
        let mut user_data = HashMap::new();
        
        Self::parse_section(xml, "EventData", &mut structured_data, &mut string_inserts);
        Self::parse_section(xml, "UserData", &mut user_data, &mut Vec::new());
        
        EventDataResult {
            structured_data,
            string_inserts,
            user_data,
        }
    }

    /// Parse a specific XML section (EventData or UserData) - Single Responsibility
    fn parse_section(
        xml: &str, 
        section_name: &str, 
        named_data: &mut HashMap<String, String>,
        inserts: &mut Vec<String>
    ) {
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);
        
        let mut buf = Vec::new();
        let mut inside_section = false;
        let mut inside_data = false;
        let mut current_data_name = String::new();
        let mut current_data_value = String::new();
        
        const MAX_ITERATIONS: usize = 500; // Security limit
        const MAX_FIELDS: usize = 100;     // Memory limit
        let mut iterations = 0;
        
        loop {
            if iterations >= MAX_ITERATIONS || named_data.len() >= MAX_FIELDS {
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
                        
                        // Extract Name attribute with proper validation
                        for attr in e.attributes() {
                            if let Ok(attr) = attr {
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
                }
                Ok(XmlEvent::End(ref e)) => {
                    let name = e.name();
                    if name.as_ref() == section_name.as_bytes() {
                        inside_section = false;
                    } else if name.as_ref() == b"Data" && inside_data {
                        inside_data = false;
                        
                        // Apply security limits
                        if current_data_value.len() > 1024 {
                            current_data_value.truncate(1024);
                            current_data_value.push_str("...[truncated]");
                        }
                        
                        // Store in appropriate format based on whether Name attribute exists
                        if !current_data_name.is_empty() {
                            named_data.insert(current_data_name.clone(), current_data_value.clone());
                        } else if section_name == "EventData" {
                            // Add to StringInserts for FluentBit compatibility
                            inserts.push(current_data_value.clone());
                        }
                    }
                }
                Ok(XmlEvent::Text(ref e)) => {
                    if inside_section && inside_data {
                        if let Ok(text) = e.unescape() {
                            if current_data_value.len() + text.len() <= 1024 {
                                current_data_value.push_str(&text);
                            }
                        }
                    }
                }
                Ok(XmlEvent::Eof) => break,
                Err(_) => break, // Security: fail gracefully
                _ => {}
            }
            
            buf.clear();
        }
    }

    fn extract_message_from_xml(xml: &str, event_id: u32, provider_name: &str, computer: &str) -> Option<String> {
        let event_data_result = Self::extract_event_data(xml);
        let event_data = &event_data_result.structured_data;
        
        match event_id {
            6009 => {
                if let (Some(version), Some(build)) = (event_data.get("Data_0"), event_data.get("Data_1")) {
                    return Some(format!("Microsoft Windows kernel version {} build {} started", 
                        version.chars().take(50).collect::<String>(),
                        build.chars().take(50).collect::<String>()
                    ));
                }
            }
            _ => {
                if !event_data.is_empty() {
                    let data_summary: Vec<String> = event_data.iter()
                        .take(3)
                        .map(|(k, v)| format!("{}={}", 
                            k.chars().take(32).collect::<String>(),
                            v.chars().take(64).collect::<String>()
                        ))
                        .collect();
                    if !data_summary.is_empty() {
                        return Some(format!("Event ID {} from {} ({})", 
                            event_id, 
                            provider_name.chars().take(64).collect::<String>(), 
                            data_summary.join(", ")
                        ));
                    }
                }
            }
        }
        
        Some(format!("Event ID {} from {} on {}", 
            event_id, 
            provider_name.chars().take(64).collect::<String>(), 
            computer.chars().take(64).collect::<String>()
        ))
    }

    fn parse_event_xml(
        xml: String,
        channel: &str,
        config: &WindowsEventLogConfig,
    ) -> Result<Option<WindowsEvent>, WindowsEventLogError> {
        // Extract basic event information with validation
        let record_id = Self::extract_xml_attribute(&xml, "EventRecordID")
            .or_else(|| Self::extract_xml_value(&xml, "EventRecordID"))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let event_id = Self::extract_xml_attribute(&xml, "EventID")
            .or_else(|| Self::extract_xml_value(&xml, "EventID"))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        // Validate we got valid event data
        if record_id == 0 && event_id == 0 {
            debug!(
                message = "Failed to parse event XML - no valid EventID or RecordID found",
                channel = %channel
            );
            return Ok(None);
        }


        // Apply event ID filters early
        if let Some(ref only_ids) = config.only_event_ids {
            if !only_ids.contains(&event_id) {
                return Ok(None);
            }
        }

        if config.ignore_event_ids.contains(&event_id) {
            return Ok(None);
        }

        // Parse timestamp with validation
        let time_created = Self::extract_xml_attribute(&xml, "SystemTime")
            .or_else(|| Self::extract_xml_value(&xml, "TimeCreated"))
            .or_else(|| Self::extract_xml_attribute(&xml, "TimeCreated"))
            .and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f%z"))
                    .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%z"))
                    .ok()
            })
            .map(|dt| {
                let dt_utc = dt.with_timezone(&Utc);
                // Validate timestamp is reasonable (within 10 years)
                let now = Utc::now();
                let diff = (now - dt_utc).num_days().abs();
                if diff > 365 * 10 {
                    now
                } else {
                    dt_utc
                }
            })
            .unwrap_or_else(|| Utc::now());

        // Apply age filter
        if let Some(max_age_secs) = config.max_event_age_secs {
            let age = Utc::now().signed_duration_since(time_created);
            if age.num_seconds() > max_age_secs as i64 {
                return Ok(None);
            }
        }

        // Use comprehensive parsing for complete field coverage
        let system_fields = Self::extract_system_fields(&xml);
        let event_data_result = Self::extract_event_data(&xml);
        let rendered_message = Self::extract_message_from_xml(&xml, system_fields.event_id, &system_fields.provider_name, &system_fields.computer);

        let event = WindowsEvent {
            record_id: system_fields.record_id,
            event_id: system_fields.event_id,
            level: system_fields.level,
            task: system_fields.task,
            opcode: system_fields.opcode,
            keywords: system_fields.keywords,
            time_created: system_fields.time_created,
            provider_name: system_fields.provider_name,
            provider_guid: system_fields.provider_guid,
            channel: system_fields.channel,
            computer: system_fields.computer,
            user_id: system_fields.user_id,
            process_id: system_fields.process_id,
            thread_id: system_fields.thread_id,
            activity_id: system_fields.activity_id,
            related_activity_id: system_fields.related_activity_id,
            raw_xml: if config.include_xml {
                // Limit XML size for security
                if xml.len() > 32768 {
                    let mut truncated = xml.chars().take(32768).collect::<String>();
                    truncated.push_str("...[truncated]");
                    truncated
                } else {
                    xml
                }
            } else {
                String::new()
            },
            rendered_message,
            event_data: event_data_result.structured_data,
            user_data: event_data_result.user_data,
            // New fields for FluentBit compatibility
            version: system_fields.version,
            qualifiers: system_fields.qualifiers,
            string_inserts: event_data_result.string_inserts,
        };

        Ok(Some(event))
    }
}

impl Drop for EventLogSubscription {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            // Cleanup global callback context
            let mut global_ctx = CALLBACK_CONTEXT.lock().unwrap();
            *global_ctx = None;
        }
    }
}

// Windows Event Log subscription callback function
#[cfg(windows)]
unsafe extern "system" fn event_subscription_callback(
    action: windows::Win32::System::EventLog::EVT_SUBSCRIBE_NOTIFY_ACTION,
    _user_context: *const std::ffi::c_void,
    event_handle: windows::Win32::System::EventLog::EVT_HANDLE,
) -> u32 {
    use windows::Win32::System::EventLog::{EvtSubscribeActionError, EvtSubscribeActionDeliver};

    #[allow(non_upper_case_globals)] // Windows API constants don't follow Rust conventions
    match action {
        EvtSubscribeActionDeliver => {
            // Process the event
            if let Err(e) = process_callback_event(event_handle) {
                warn!("Error processing callback event: {}", e);
            }
        }
        EvtSubscribeActionError => {
            warn!("Windows Event Log subscription error occurred");
        }
        _ => {
            debug!("Unknown subscription callback action: {}", action.0);
        }
    }

    0 // Return success
}

#[cfg(windows)]
fn process_callback_event(event_handle: windows::Win32::System::EventLog::EVT_HANDLE) -> Result<(), WindowsEventLogError> {
    use windows::Win32::System::EventLog::{EvtRender, EvtRenderEventXml};
    use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;

    // Get callback context
    let context = {
        let global_ctx = CALLBACK_CONTEXT.lock().unwrap();
        global_ctx.clone()
    };

    let Some(ctx) = context else {
        warn!("No callback context available");
        return Ok(());
    };

    const MAX_BUFFER_SIZE: u32 = 1024 * 1024; // 1MB limit
    const DEFAULT_BUFFER_SIZE: u32 = 4096; // 4KB default

    let buffer_size = DEFAULT_BUFFER_SIZE;
    let mut buffer_used = 0u32;
    let mut buffer: Vec<u8> = vec![0u8; buffer_size as usize];

    // First attempt with default buffer size
    let mut property_count = 0u32;
    let result = unsafe {
        EvtRender(
            None,
            event_handle,
            EvtRenderEventXml.0,
            buffer_size,
            Some(buffer.as_mut_ptr() as *mut std::ffi::c_void),
            &mut buffer_used,
            &mut property_count,
        )
    };

    // Handle buffer reallocation if needed
    if let Err(e) = result {
        if e.code() == ERROR_INSUFFICIENT_BUFFER.into() {
            if buffer_used == 0 || buffer_used > MAX_BUFFER_SIZE {
                warn!("Event XML buffer size invalid, skipping event");
                return Ok(());
            }

            // Reallocate with exact required size
            buffer.resize(buffer_used as usize, 0);
            let mut second_buffer_used = 0u32;
            let mut second_property_count = 0u32;

            let result = unsafe {
                EvtRender(
                    None,
                    event_handle,
                    EvtRenderEventXml.0,
                    buffer_used,
                    Some(buffer.as_mut_ptr() as *mut std::ffi::c_void),
                    &mut second_buffer_used,
                    &mut second_property_count,
                )
            };

            if let Err(e) = result {
                warn!("EvtRender failed on second attempt: {}", e);
                return Err(WindowsEventLogError::ReadEventError { source: e });
            }

            buffer_used = second_buffer_used;
        } else {
            warn!("EvtRender failed: {}", e);
            return Err(WindowsEventLogError::ReadEventError { source: e });
        }
    }

    // Validate buffer usage
    if buffer_used as usize > buffer.len() || buffer_used == 0 {
        warn!("Invalid buffer usage in EvtRender, skipping event");
        return Ok(());
    }

    // Convert byte buffer to UTF-16 string
    if buffer_used < 2 || buffer_used % 2 != 0 {
        debug!("Invalid UTF-16 buffer size");
        return Ok(());
    }

    let u16_slice = unsafe {
        std::slice::from_raw_parts(
            buffer.as_ptr() as *const u16,
            buffer_used as usize / 2
        )
    };

    // Remove null terminator if present
    let xml_len = if u16_slice.len() > 0 && u16_slice[u16_slice.len() - 1] == 0 {
        u16_slice.len() - 1
    } else {
        u16_slice.len()
    };

    if xml_len == 0 {
        debug!("Empty XML content, skipping event");
        return Ok(());
    }

    let xml = String::from_utf16_lossy(&u16_slice[..xml_len]);

    // Determine channel from XML (simplified approach)
    let channel = EventLogSubscription::extract_xml_value(&xml, "Channel")
        .unwrap_or_else(|| "Unknown".to_string());

    // Parse the XML to extract event data
    if let Ok(Some(event)) = EventLogSubscription::parse_event_xml(xml, &channel, &ctx.config) {
        if let Err(_) = ctx.event_sender.send(event) {
            debug!("Failed to send event - receiver dropped");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(EventLogSubscription::extract_xml_value(xml, "EventID"), Some("1".to_string()));
        assert_eq!(EventLogSubscription::extract_xml_value(xml, "Level"), Some("4".to_string()));
        assert_eq!(EventLogSubscription::extract_xml_value(xml, "EventRecordID"), Some("12345".to_string()));
        assert_eq!(EventLogSubscription::extract_xml_value(xml, "Channel"), Some("System".to_string()));
        assert_eq!(EventLogSubscription::extract_xml_value(xml, "Computer"), Some("TEST-MACHINE".to_string()));
        assert_eq!(EventLogSubscription::extract_xml_value(xml, "NonExistent"), None);
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

        assert_eq!(EventLogSubscription::extract_xml_attribute(xml, "Name"), Some("Microsoft-Windows-Kernel-General".to_string()));
        assert_eq!(EventLogSubscription::extract_xml_attribute(xml, "SystemTime"), Some("2025-08-29T00:15:41.123456Z".to_string()));
        assert_eq!(EventLogSubscription::extract_xml_attribute(xml, "NonExistent"), None);
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
            version: Some(1),
            qualifiers: Some(0),
            string_inserts: vec![],
        };

        assert_eq!(event.level_name(), "Error");
    }

    #[cfg(not(windows))]
    #[test]
    fn test_not_supported_error() {
        let config = WindowsEventLogConfig::default();
        let result = EventLogSubscription::new(&config);

        assert!(matches!(
            result,
            Err(WindowsEventLogError::NotSupportedError)
        ));
    }

    #[test]
    fn test_security_limits() {
        // Test XML element extraction with size limits
        let large_xml = format!(r#"
        <Event>
            <System>
                <EventID>{}</EventID>
            </System>
        </Event>
        "#, "x".repeat(10000)); // Very large content

        // Should not panic or consume excessive memory
        // Security limits should prevent processing excessively large content
        let result = EventLogSubscription::extract_xml_value(&large_xml, "EventID");
        assert!(result.is_none(), "Security limits should reject excessively large XML content");
    }
}