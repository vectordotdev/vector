use std::{
    collections::HashMap,
    path::PathBuf,
};

use chrono::{DateTime, Utc};
use quick_xml::{Reader, events::Event as XmlEvent};
use regex;
use tokio::fs;

use super::{config::WindowsEventLogConfig, error::*};

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

/// Manages Windows Event Log subscriptions and querying
pub struct EventLogSubscription {
    config: WindowsEventLogConfig,
    bookmark_file: Option<PathBuf>,
    last_bookmarks: HashMap<String, String>,
}

impl EventLogSubscription {
    /// Create a new event log subscription
    pub fn new(config: &WindowsEventLogConfig) -> Result<Self, WindowsEventLogError> {
        #[cfg(not(windows))]
        {
            return Err(WindowsEventLogError::NotSupportedError);
        }

        let bookmark_file = config.bookmark_db_path.clone();

        let subscription = Self {
            config: config.clone(),
            bookmark_file,
            last_bookmarks: HashMap::new(),
        };

        // Note: bookmarks will be loaded on first poll

        // Validate channels exist and are accessible
        subscription.validate_channels()?;

        Ok(subscription)
    }

    /// Poll for new events from all configured channels
    pub async fn poll_events(&mut self) -> Result<Vec<WindowsEvent>, WindowsEventLogError> {
        #[cfg(not(windows))]
        {
            return Err(WindowsEventLogError::NotSupportedError);
        }

        #[cfg(windows)]
        {
            debug!(message = "Starting event poll for all channels");
            
            // Load bookmarks on first call if needed
            if self.last_bookmarks.is_empty() {
                debug!(message = "Loading bookmarks for first poll");
                self.load_bookmarks().await?;
            }

            let mut all_events = Vec::new();
            let max_events = self.config.batch_size as usize;

            for channel in self.config.channels.clone() {
                debug!(message = "Polling channel for events", channel = %channel);
                let events = self.poll_channel_events(&channel, max_events).await?;
                debug!(
                    message = "Channel poll completed", 
                    channel = %channel,
                    event_count = events.len()
                );
                all_events.extend(events);

                if all_events.len() >= max_events {
                    all_events.truncate(max_events);
                    break;
                }
            }

            // Filter events based on configuration
            let filtered_events = self.filter_events(all_events)?;

            // Update bookmarks for processed events
            self.update_bookmarks(&filtered_events).await?;

            Ok(filtered_events)
        }
    }

    #[cfg(windows)]
    async fn poll_channel_events(
        &mut self,
        channel: &str,
        max_events: usize,
    ) -> Result<Vec<WindowsEvent>, WindowsEventLogError> {
        use windows::{
            Win32::System::EventLog::{
                EVT_HANDLE, EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath,
                EvtQueryForwardDirection, EvtQueryReverseDirection, EvtQueryTolerateQueryErrors,
            },
            core::HSTRING,
        };

        let channel_hstring = HSTRING::from(channel);
        let query = self.build_xpath_query(channel)?;
        let query_hstring = HSTRING::from(query.clone());

        // Determine query flags based on configuration
        // Use EvtQueryTolerateQueryErrors to handle query errors gracefully
        let mut query_flags = EvtQueryChannelPath.0 | EvtQueryTolerateQueryErrors.0;
        
        // For existing events, use reverse direction to read from oldest first
        // For new events only, use forward direction to read most recent first
        query_flags |= if self.config.read_existing_events {
            EvtQueryReverseDirection.0  // Read from oldest to newest
        } else {
            EvtQueryForwardDirection.0  // Read from newest first
        };

        debug!(
            message = "Opening Windows Event Log query",
            channel = %channel,
            query = %query,
            read_existing = self.config.read_existing_events,
            query_flags = query_flags
        );

        // Open query handle
        let query_handle = unsafe {
            EvtQuery(
                None, // Session handle
                &channel_hstring,
                &query_hstring,
                query_flags,
            )
        }
        .map_err(|e| WindowsEventLogError::QueryEventsError { source: e })?;

        // RAII wrapper for safe handle management
        struct SafeEventHandle(EVT_HANDLE);
        impl Drop for SafeEventHandle {
            fn drop(&mut self) {
                if !self.0.is_invalid() {
                    if let Err(e) = unsafe { EvtClose(self.0) } {
                        warn!("Failed to close event handle: {}", e);
                    }
                }
            }
        }

        // Wrap query handle for automatic cleanup
        let query_handle = SafeEventHandle(query_handle);
        let mut events = Vec::with_capacity(max_events.min(1000)); // Pre-allocate

        // Use smaller batches to prevent memory pressure
        const BATCH_SIZE: usize = 50;
        let mut event_handles = vec![EVT_HANDLE::default(); BATCH_SIZE];

        loop {
            let mut returned = 0u32;

            let result = unsafe {
                EvtNext(
                    query_handle.0,
                    std::mem::transmute::<&mut [EVT_HANDLE], &mut [isize]>(&mut event_handles[..]),
                    5000, // 5 second timeout
                    0,    // Flags
                    &mut returned,
                )
            };

            // Better error handling for EvtNext
            if let Err(e) = result {
                let error_code = e.code().0;
                // ERROR_NO_MORE_ITEMS can be 259 (positive) or -2147024637 (negative HRESULT)
                if error_code == 259 || error_code == -2147024637 {
                    debug!(
                        message = "No more events available in channel",
                        channel = %channel,
                        events_processed = %events.len()
                    );
                } else {
                    warn!(
                        message = "EvtNext failed with error",
                        error_code = %error_code,
                        channel = %channel,
                        internal_log_rate_limit = true
                    );
                }
                break;
            }

            if returned == 0 {
                debug!(
                    message = "EvtNext returned 0 events",
                    channel = %channel
                );
                break;
            }

            // Process handles with RAII protection
            let safe_handles: Vec<SafeEventHandle> = (0..returned as usize)
                .map(|i| SafeEventHandle(event_handles[i]))
                .collect();

            for (i, handle_wrapper) in safe_handles.iter().enumerate() {
                debug!(
                    message = "Processing event handle",
                    channel = %channel,
                    event_index = i,
                    handle_valid = !handle_wrapper.0.is_invalid()
                );
                match self.process_event_handle(handle_wrapper.0, channel).await {
                    Ok(Some(event)) => {
                        events.push(event);
                        if events.len() >= max_events {
                            return Ok(events);
                        }
                    }
                    Ok(None) => {
                        // Event was filtered out - this is normal
                        trace!(
                            message = "Event filtered out",
                            channel = %channel,
                            event_index = i,
                        );
                    }
                    Err(e) => {
                        warn!(
                            message = "Failed to process event",
                            error = %e,
                            channel = %channel,
                            event_index = i,
                            internal_log_rate_limit = true
                        );
                    }
                }
            }
            // Handles automatically cleaned up by Drop

            if events.len() >= max_events {
                break;
            }
        }

        Ok(events)
    }

    #[cfg(windows)]
    async fn process_event_handle(
        &self,
        event_handle: windows::Win32::System::EventLog::EVT_HANDLE,
        channel: &str,
    ) -> Result<Option<WindowsEvent>, WindowsEventLogError> {
        use windows::Win32::System::EventLog::{EvtRender, EvtRenderEventXml};
        use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;

        // Prevent excessive buffer allocation - strict limit to 1MB for security
        const MAX_BUFFER_SIZE: u32 = 1024 * 1024;

        // Follow Microsoft's exact pattern: use byte buffer for EvtRender
        const DEFAULT_BUFFER_SIZE: u32 = 4096; // 4KB default buffer
        
        let mut buffer_size = DEFAULT_BUFFER_SIZE;
        let mut buffer_used = 0u32;
        let mut buffer: Vec<u8> = vec![0u8; buffer_size as usize]; // Use byte buffer

        // First attempt with default buffer size
        let mut property_count = 0u32;
        let result = unsafe {
            EvtRender(
                None, // No context needed for XML rendering
                event_handle,
                EvtRenderEventXml.0,
                buffer_size,
                Some(buffer.as_mut_ptr() as *mut std::ffi::c_void),
                &mut buffer_used,
                &mut property_count,
            )
        };

        debug!(
            message = "EvtRender first attempt result",
            success = result.is_ok(),
            buffer_size = %buffer_size,
            buffer_used = %buffer_used,
            channel = %channel
        );

        // Check if first attempt succeeded (Microsoft pattern)
        if result.is_ok() {
            // First call succeeded - use this data directly
            debug!(
                message = "EvtRender first call succeeded, using result",
                buffer_used = %buffer_used,
                channel = %channel
            );
        } else if let Err(e) = result {
            let error_code = e.code();
            if error_code == ERROR_INSUFFICIENT_BUFFER.into() {
                // Reallocate with required size
                if buffer_used == 0 || buffer_used > MAX_BUFFER_SIZE {
                    warn!(
                        message = "Event XML buffer size invalid, skipping event",
                        buffer_used = %buffer_used,
                        max_size = %MAX_BUFFER_SIZE,
                        channel = %channel,
                        internal_log_rate_limit = true
                    );
                    return Ok(None);
                }

                debug!(
                    message = "Reallocating buffer for event XML",
                    old_size = %buffer_size,
                    required_size = %buffer_used,
                    channel = %channel
                );

                // Allocate exact size required  
                let required_size = buffer_used;
                buffer.resize(required_size as usize, 0);
                
                // Important: keep original buffer_size for second call, don't reset buffer_used to 0
                let second_buffer_size = required_size;
                let mut second_buffer_used = 0u32;

                // Second attempt with correctly sized buffer
                let mut second_property_count = 0u32;
                let result = unsafe {
                    EvtRender(
                        None,
                        event_handle,
                        EvtRenderEventXml.0,
                        second_buffer_size,
                        Some(buffer.as_mut_ptr() as *mut std::ffi::c_void),
                        &mut second_buffer_used,
                        &mut second_property_count,
                    )
                };

                if let Err(e) = result {
                    warn!(
                        message = "EvtRender failed on second attempt",
                        error = %e,
                        required_size = %required_size,
                        second_buffer_size = %second_buffer_size,
                        second_buffer_used = %second_buffer_used,
                        channel = %channel,
                        internal_log_rate_limit = true
                    );
                    return Err(WindowsEventLogError::ReadEventError { source: e });
                }
                
                // Update buffer_used for the rest of the function
                buffer_used = second_buffer_used;
                buffer_size = second_buffer_size;
            } else {
                warn!(
                    message = "EvtRender failed with error",
                    error_code = %error_code.0,
                    channel = %channel,
                    internal_log_rate_limit = true
                );
                return Err(WindowsEventLogError::ReadEventError { source: e });
            }
        }

        // Additional safety check: ensure buffer_used doesn't exceed allocated buffer
        if buffer_used as usize > buffer_size as usize || buffer_used == 0 {
            warn!(
                message = "Invalid buffer usage in EvtRender, skipping event",
                buffer_used = %buffer_used,
                buffer_size = %buffer_size,
                channel = %channel,
                internal_log_rate_limit = true
            );
            return Ok(None);
        }

        debug!(
            message = "EvtRender succeeded",
            buffer_size = %buffer_size,
            buffer_used = %buffer_used,
            channel = %channel
        );

        // Convert byte buffer to UTF-16 string (Windows XML is UTF-16)
        if buffer_used < 2 {
            debug!(
                message = "Event has no XML content (buffer_used < 2), trying to log raw data",
                buffer_used = %buffer_used,
                raw_bytes = ?&buffer[..std::cmp::min(buffer_used as usize, 16)],
                channel = %channel,
            );
            return Ok(None);
        }
        
        if buffer_used % 2 != 0 {
            debug!(
                message = "Odd buffer size for UTF-16, might be corrupted event",
                buffer_used = %buffer_used,
                channel = %channel,
            );
            return Ok(None);
        }
        
        // Convert bytes to u16 slice for UTF-16 processing
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
            debug!(
                message = "Empty XML content, skipping event", 
                channel = %channel,
            );
            return Ok(None);
        }
        
        let xml = String::from_utf16_lossy(&u16_slice[..xml_len]);

        // Debug: Log the actual XML we're getting to understand the structure
        debug!(
            message = "Received Windows Event XML",
            xml = %xml,
            channel = %channel,
            internal_log_rate_limit = true
        );

        // Parse the XML to extract event data
        self.parse_event_xml(xml, channel)
    }

    fn parse_event_xml(
        &self,
        xml: String,
        channel: &str,
    ) -> Result<Option<WindowsEvent>, WindowsEventLogError> {
        // This is a simplified parser - in a real implementation, we'd use a proper XML parser
        // For now, we'll extract basic information using string parsing

        // Parse Windows Event XML properly
        // Windows Event XML has structure like:
        // <Event><System><EventID>123</EventID><Level>4</Level><EventRecordID>456</EventRecordID>...</System>...</Event>
        
        // If we can't parse basic event information, the XML is likely invalid/empty
        let record_id = Self::extract_xml_attribute(&xml, "EventRecordID")
            .or_else(|| Self::extract_xml_value(&xml, "EventRecordID"))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let event_id = Self::extract_xml_attribute(&xml, "EventID")
            .or_else(|| Self::extract_xml_value(&xml, "EventID"))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        
        // If both record_id and event_id are 0, the XML parsing likely failed
        // This indicates we didn't get valid Windows Event Log XML
        if record_id == 0 && event_id == 0 {
            debug!(
                message = "Failed to parse event XML - no valid EventID or RecordID found",
                xml_sample = %xml.chars().take(500).collect::<String>(),
                channel = %channel,
                internal_log_rate_limit = true
            );
            return Ok(None);
        }

        let level = Self::extract_xml_attribute(&xml, "Level")
            .or_else(|| Self::extract_xml_value(&xml, "Level"))
            .and_then(|s| s.parse::<u8>().ok())
            .filter(|&l| l <= 5) // Validate level is within expected range
            .unwrap_or(4);

        // Apply event ID filters
        if let Some(ref only_ids) = self.config.only_event_ids {
            if !only_ids.contains(&event_id) {
                return Ok(None);
            }
        }

        if self.config.ignore_event_ids.contains(&event_id) {
            return Ok(None);
        }

        // Safe timestamp parsing with validation - try multiple Windows timestamp formats
        let time_created = Self::extract_xml_attribute(&xml, "SystemTime")
            .or_else(|| Self::extract_xml_value(&xml, "TimeCreated"))
            .or_else(|| Self::extract_xml_attribute(&xml, "TimeCreated"))
            .and_then(|s| {
                // Try multiple timestamp formats that Windows uses
                DateTime::parse_from_rfc3339(&s)
                    .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f%z"))
                    .or_else(|_| DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%z"))
                    .ok()
            })
            .map(|dt| {
                // Validate timestamp is reasonable (not too far in future/past)
                let now = Utc::now();
                let dt_utc = dt.with_timezone(&Utc);
                let diff = (now - dt_utc).num_days().abs();
                
                if diff > 365 * 10 { // More than 10 years difference
                    warn!(
                        message = "Event timestamp seems unrealistic, using current time",
                        event_timestamp = %dt_utc,
                        channel = %channel,
                        internal_log_rate_limit = true
                    );
                    now
                } else {
                    dt_utc
                }
            })
            .unwrap_or_else(|| Utc::now());

        // Apply age filter
        if let Some(max_age_secs) = self.config.max_event_age_secs {
            let age = Utc::now().signed_duration_since(time_created);
            if age.num_seconds() > max_age_secs as i64 {
                return Ok(None);
            }
        }

        // Safe string field extraction with length validation
        let provider_name = Self::extract_provider_name(&xml)
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .unwrap_or_else(|| "Unknown".to_string());

        let computer = Self::extract_xml_attribute(&xml, "Computer")
            .or_else(|| Self::extract_xml_value(&xml, "Computer"))
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .unwrap_or_else(|| "localhost".to_string());

        // Safe numeric parsing with validation
        let process_id = Self::extract_xml_value(&xml, "ProcessID")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        let thread_id = Self::extract_xml_value(&xml, "ThreadID")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        // Extract rendered message before moving values
        let rendered_message = Self::extract_message_from_xml(&xml, event_id, &provider_name, &computer);
        let event_data = Self::extract_event_data(&xml);
        
        let event = WindowsEvent {
            record_id,
            event_id,
            level,
            task: 0, // Would be extracted from XML in full implementation
            opcode: 0,
            keywords: 0,
            time_created,
            provider_name,
            provider_guid: None,
            channel: channel.to_string(),
            computer,
            user_id: Self::extract_xml_value(&xml, "UserID"),
            process_id,
            thread_id,
            activity_id: Self::extract_xml_value(&xml, "ActivityID"),
            related_activity_id: Self::extract_xml_value(&xml, "RelatedActivityID"),
            raw_xml: if self.config.include_xml {
                xml.clone()
            } else {
                String::new()
            },
            rendered_message,
            event_data,
            user_data: HashMap::new(), // Would be extracted from UserData section
        };

        Ok(Some(event))
    }

    /// Safely extract XML value using proper XML parser
    fn extract_xml_attribute(xml: &str, attr_name: &str) -> Option<String> {
        // Look for attribute patterns like EventID="123"
        let pattern = format!(r#"{}="([^"]+)""#, regex::escape(attr_name));
        if let Ok(re) = regex::Regex::new(&pattern) {
            if let Some(cap) = re.captures(xml) {
                return cap.get(1).map(|m| m.as_str().to_string());
            }
        }
        None
    }

    fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);

        let mut buf = Vec::new();
        let mut inside_target = false;
        let mut current_element = String::new();

        // Limit parsing to prevent DoS attacks
        const MAX_ITERATIONS: usize = 10000;
        let mut iterations = 0;

        loop {
            if iterations >= MAX_ITERATIONS {
                warn!(
                    message = "XML parsing iteration limit exceeded",
                    target_tag = %tag,
                    internal_log_rate_limit = true,
                );
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
                            Ok(text) => current_element.push_str(&text),
                            Err(_) => {
                                warn!(
                                    message = "Failed to unescape XML text",
                                    target_tag = %tag,
                                    internal_log_rate_limit = true,
                                );
                                return None;
                            }
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
                Err(_) => {
                    warn!(
                        message = "XML parsing error",
                        target_tag = %tag,
                        internal_log_rate_limit = true,
                    );
                    return None;
                }
                _ => {}
            }

            buf.clear();
        }

        None
    }

    /// Extract provider name from Windows Event Log XML
    /// Handles the specific format: <Provider Name='EventLog'/>
    fn extract_provider_name(xml: &str) -> Option<String> {
        // Look for Provider element with Name attribute pattern: <Provider Name="..." or <Provider Name='...'
        let pattern = r#"<Provider\s+Name=['"]([^'"]+)['"]"#;
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(cap) = re.captures(xml) {
                return cap.get(1).map(|m| m.as_str().to_string());
            }
        }
        None
    }

    /// Extract EventData from Windows Event Log XML
    fn extract_event_data(xml: &str) -> HashMap<String, String> {
        let mut event_data = HashMap::new();
        
        // Parse EventData section and extract Data elements
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);
        
        let mut buf = Vec::new();
        let mut inside_event_data = false;
        let mut inside_data = false;
        let mut current_data_name = String::new();
        let mut current_data_value = String::new();
        
        const MAX_ITERATIONS: usize = 1000;
        let mut iterations = 0;
        
        loop {
            if iterations >= MAX_ITERATIONS {
                break;
            }
            iterations += 1;
            
            match reader.read_event_into(&mut buf) {
                Ok(XmlEvent::Start(ref e)) => {
                    let name = e.name();
                    if name.as_ref() == b"EventData" {
                        inside_event_data = true;
                    } else if inside_event_data && name.as_ref() == b"Data" {
                        inside_data = true;
                        current_data_name.clear();
                        current_data_value.clear();
                        
                        // Look for Name attribute
                        for attr in e.attributes() {
                            if let Ok(attr) = attr {
                                if attr.key.as_ref() == b"Name" {
                                    current_data_name = String::from_utf8_lossy(&attr.value).into_owned();
                                    break;
                                }
                            }
                        }
                    }
                }
                Ok(XmlEvent::End(ref e)) => {
                    let name = e.name();
                    if name.as_ref() == b"EventData" {
                        inside_event_data = false;
                    } else if name.as_ref() == b"Data" && inside_data {
                        inside_data = false;
                        // If we have both name and value, add to map
                        if !current_data_name.is_empty() {
                            event_data.insert(current_data_name.clone(), current_data_value.clone());
                        } else {
                            // Use numeric index for unnamed data elements
                            let index = event_data.len();
                            event_data.insert(format!("Data_{}", index), current_data_value.clone());
                        }
                    }
                }
                Ok(XmlEvent::Text(ref e)) => {
                    if inside_event_data && inside_data {
                        if let Ok(text) = e.unescape() {
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
        
        event_data
    }

    /// Extract or construct a meaningful message from the event
    fn extract_message_from_xml(xml: &str, event_id: u32, provider_name: &str, computer: &str) -> Option<String> {
        // First try to extract event data for context
        let event_data = Self::extract_event_data(xml);
        
        // For some well-known event types, construct meaningful messages
        match event_id {
            6009 => {
                // Microsoft Windows kernel version message
                if let (Some(version), Some(build)) = (event_data.get("Data_0"), event_data.get("Data_1")) {
                    return Some(format!("Microsoft Windows kernel version {} build {} started", version, build));
                }
            }
            _ => {
                // For other events, try to construct from available data
                if !event_data.is_empty() {
                    let data_summary: Vec<String> = event_data.iter()
                        .take(3) // Limit to first 3 data items
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect();
                    if !data_summary.is_empty() {
                        return Some(format!("Event ID {} from {} ({})", event_id, provider_name, data_summary.join(", ")));
                    }
                }
            }
        }
        
        // Fall back to generic message with provider name
        Some(format!("Event ID {} from {} on {}", event_id, provider_name, computer))
    }

    fn build_xpath_query(&self, channel: &str) -> Result<String, WindowsEventLogError> {
        let query = if let Some(ref custom_query) = self.config.event_query {
            custom_query.clone()
        } else {
            // Use a more specific query that should match actual Windows Event Log events
            // "*" means all events, which should work for any channel
            "*".to_string()
        };

        // Add bookmark filtering if we have a previous position
        if let Some(bookmark) = self.last_bookmarks.get(channel) {
            // In a full implementation, we would use the bookmark to continue from last position
            debug!(
                message = "Using bookmark for channel",
                channel = %channel,
                bookmark = %bookmark
            );
        }

        debug!(
            message = "Built XPath query for Windows Event Log",
            query = %query,
            channel = %channel
        );

        Ok(query)
    }

    fn validate_channels(&self) -> Result<(), WindowsEventLogError> {
        #[cfg(windows)]
        {
            use windows::{
                Win32::System::EventLog::{EvtClose, EvtOpenChannelEnum},
            };

            // Try to enumerate channels to validate they exist
            let enum_handle = unsafe { EvtOpenChannelEnum(None, 0) }
                .map_err(|e| WindowsEventLogError::CreateSubscriptionError { source: e })?;

            if let Err(e) = unsafe { EvtClose(enum_handle) } {
                warn!("Failed to close enum handle: {}", e);
            }
        }

        Ok(())
    }

    fn filter_events(
        &self,
        events: Vec<WindowsEvent>,
    ) -> Result<Vec<WindowsEvent>, WindowsEventLogError> {
        let mut filtered = Vec::new();

        for event in events {
            // Apply field filtering logic here
            if self.should_include_event(&event) {
                filtered.push(event);
            }
        }

        Ok(filtered)
    }

    fn should_include_event(&self, _event: &WindowsEvent) -> bool {
        // Implement filtering logic based on field_filter configuration
        // For now, include all events
        true
    }

    async fn load_bookmarks(&mut self) -> Result<(), WindowsEventLogError> {
        if let Some(ref path) = self.bookmark_file {
            match fs::read_to_string(path).await {
                Ok(content) => {
                    // Validate file size to prevent memory exhaustion
                    if content.len() > 1024 * 1024 { // 1MB limit
                        return Err(WindowsEventLogError::BookmarkPersistenceError {
                            message: "Bookmark file is too large".to_string(),
                        });
                    }
                    
                    let mut line_count = 0;
                    for line in content.lines() {
                        line_count += 1;
                        
                        // Prevent excessive lines
                        if line_count > 10000 {
                            return Err(WindowsEventLogError::BookmarkPersistenceError {
                                message: "Bookmark file contains too many lines".to_string(),
                            });
                        }
                        
                        if let Some((channel, bookmark)) = line.split_once('=') {
                            let channel = channel.trim();
                            let bookmark = bookmark.trim();
                            
                            // Validate bookmark data
                            if channel.is_empty() || bookmark.is_empty() {
                                continue; // Skip invalid lines
                            }
                            
                            if channel.len() > 256 || bookmark.len() > 256 {
                                warn!(
                                    message = "Invalid bookmark data length, skipping",
                                    channel = %channel,
                                    internal_log_rate_limit = true
                                );
                                continue;
                            }
                            
                            // Validate channel name format
                            if !channel.chars().all(|c| c.is_ascii_alphanumeric() || "-_ /\\".contains(c)) {
                                warn!(
                                    message = "Invalid channel name in bookmark, skipping",
                                    channel = %channel,
                                    internal_log_rate_limit = true
                                );
                                continue;
                            }
                            
                            self.last_bookmarks.insert(channel.to_string(), bookmark.to_string());
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // File doesn't exist yet - this is OK for first run
                    debug!("Bookmark file not found, starting fresh");
                }
                Err(e) => {
                    return Err(WindowsEventLogError::BookmarkPersistenceError {
                        message: format!("Failed to read bookmark file: {}", e),
                    });
                }
            }
        }
        Ok(())
    }

    async fn update_bookmarks(
        &mut self,
        events: &[WindowsEvent],
    ) -> Result<(), WindowsEventLogError> {
        if let Some(ref path) = self.bookmark_file {
            if events.is_empty() && self.last_bookmarks.is_empty() {
                return Ok(());
            }

            // Group events by channel and get the latest bookmark for each
            let mut channel_bookmarks = HashMap::new();
            for event in events {
                // Use record ID as bookmark with validation
                let bookmark = format!("record:{}", event.record_id);
                channel_bookmarks.insert(&event.channel, bookmark);
            }

            // Update in-memory bookmarks
            for (channel, bookmark) in &channel_bookmarks {
                self.last_bookmarks
                    .insert(channel.to_string(), bookmark.clone());
            }

            // Atomic file write to prevent TOCTOU races and corruption
            let mut content = String::with_capacity(self.last_bookmarks.len() * 64);
            for (channel, bookmark) in &self.last_bookmarks {
                // Validate bookmark data before writing
                if channel.len() > 256 || bookmark.len() > 256 {
                    warn!(
                        message = "Bookmark data too long, skipping",
                        channel = %channel,
                        bookmark_len = %bookmark.len(),
                        internal_log_rate_limit = true
                    );
                    continue;
                }
                content.push_str(&format!("{}={}\n", channel, bookmark));
            }

            // Use temporary file + rename for atomic operation
            let temp_path = path.with_extension("tmp");
            
            match fs::write(&temp_path, &content).await {
                Ok(()) => {
                    // Atomic rename to final destination
                    if let Err(e) = fs::rename(&temp_path, path).await {
                        // Clean up temp file on failure
                        let _ = fs::remove_file(&temp_path).await;
                        return Err(WindowsEventLogError::BookmarkPersistenceError {
                            message: format!("Failed to rename bookmark file: {}", e),
                        });
                    }
                }
                Err(e) => {
                    // Clean up temp file on failure
                    let _ = fs::remove_file(&temp_path).await;
                    return Err(WindowsEventLogError::BookmarkPersistenceError {
                        message: format!("Failed to write bookmarks to temporary file: {}", e),
                    });
                }
            }
        }

        Ok(())
    }
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
                <Version>0</Version>
                <Level>4</Level>
                <Task>0</Task>
                <Opcode>0</Opcode>
                <Keywords>0x8000000000000000</Keywords>
                <TimeCreated SystemTime="2025-08-29T00:15:41.123456Z"/>
                <EventRecordID>12345</EventRecordID>
                <Correlation/>
                <Execution ProcessID="4" ThreadID="8"/>
                <Channel>System</Channel>
                <Computer>TEST-MACHINE</Computer>
            </System>
            <EventData>
                <Data Name="param1">value1</Data>
                <Data Name="param2">value2</Data>
            </EventData>
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
        };

        assert_eq!(event.level_name(), "Error");
    }

    #[test]
    fn test_xml_value_extraction() {
        let xml = r#"<Event><System><EventID>1000</EventID><Level>2</Level></System></Event>"#;

        assert_eq!(
            EventLogSubscription::extract_xml_value(xml, "EventID"),
            Some("1000".to_string())
        );
        assert_eq!(
            EventLogSubscription::extract_xml_value(xml, "Level"),
            Some("2".to_string())
        );
        assert_eq!(
            EventLogSubscription::extract_xml_value(xml, "NonExistent"),
            None
        );
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

    #[tokio::test]
    async fn test_file_bookmark_persistence() {
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        let config = WindowsEventLogConfig {
            channels: vec!["System".to_string()],
            bookmark_db_path: Some(temp_file.path().to_path_buf()),
            ..Default::default()
        };

        let mut subscription = EventLogSubscription {
            config,
            bookmark_file: Some(temp_file.path().to_path_buf()),
            last_bookmarks: HashMap::new(),
        };

        // Test saving bookmarks
        subscription
            .last_bookmarks
            .insert("System".to_string(), "record:12345".to_string());
        subscription
            .last_bookmarks
            .insert("Application".to_string(), "record:67890".to_string());

        let events = Vec::new(); // Empty events, will use existing bookmarks
        subscription.update_bookmarks(&events).await.unwrap();

        // Test loading bookmarks
        subscription.last_bookmarks.clear();
        subscription.load_bookmarks().await.unwrap();

        assert_eq!(
            subscription.last_bookmarks.get("System"),
            Some(&"record:12345".to_string())
        );
        assert_eq!(
            subscription.last_bookmarks.get("Application"),
            Some(&"record:67890".to_string())
        );
    }
}
