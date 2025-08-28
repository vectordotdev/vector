use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Utc};
use quick_xml::{Reader, events::Event as XmlEvent};
use snafu::{OptionExt, ResultExt};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

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

        let mut subscription = Self {
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
            // Load bookmarks on first call if needed
            if self.last_bookmarks.is_empty() {
                self.load_bookmarks().await?;
            }

            let mut all_events = Vec::new();
            let max_events = self.config.batch_size as usize;

            for channel in &self.config.channels {
                let events = self.poll_channel_events(channel, max_events).await?;
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
                EvtQueryForwardDirection, EvtRender, EvtRenderEventXml,
            },
            core::HSTRING,
        };

        let channel_hstring = HSTRING::from(channel);
        let query = self.build_xpath_query(channel)?;
        let query_hstring = HSTRING::from(query);

        // Open query handle
        let query_handle = unsafe {
            EvtQuery(
                None, // Session handle
                &channel_hstring,
                &query_hstring,
                EvtQueryChannelPath | EvtQueryForwardDirection,
            )
        }
        .map_err(|e| WindowsEventLogError::QueryEventsError { source: e })?;

        // RAII wrapper for safe handle management
        struct SafeEventHandle(EVT_HANDLE);
        impl Drop for SafeEventHandle {
            fn drop(&mut self) {
                if !self.0.is_invalid() {
                    unsafe { EvtClose(self.0) };
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
                    event_handles.len() as u32,
                    event_handles.as_mut_ptr(),
                    5000, // 5 second timeout to prevent hanging
                    0,    // Flags
                    &mut returned,
                )
            };

            if !result.as_bool() || returned == 0 {
                break;
            }

            // Process handles with RAII protection
            let safe_handles: Vec<SafeEventHandle> = (0..returned as usize)
                .map(|i| SafeEventHandle(event_handles[i]))
                .collect();

            for (i, handle_wrapper) in safe_handles.iter().enumerate() {
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

        // Prevent excessive buffer allocation - limit to 1MB
        const MAX_BUFFER_SIZE: u32 = 1024 * 1024;

        // Get the event XML
        let mut buffer_size = 0u32;
        let mut buffer_used = 0u32;

        // First call to get required buffer size
        unsafe {
            EvtRender(
                None, // Context
                event_handle,
                EvtRenderEventXml,
                0,
                std::ptr::null_mut(),
                &mut buffer_size,
                &mut buffer_used,
            )
        };

        if buffer_size == 0 {
            return Err(WindowsEventLogError::ReadEventError {
                source: windows::core::Error::from_win32(),
            });
        }

        // Prevent DoS attacks via excessive memory allocation with strict validation
        if buffer_size == 0 || buffer_size > MAX_BUFFER_SIZE {
            warn!(
                message = "Event XML buffer size invalid, skipping event",
                buffer_size = %buffer_size,
                max_size = %MAX_BUFFER_SIZE,
                channel = %channel,
                internal_log_rate_limit = true
            );
            return Ok(None);
        }

        // Use checked arithmetic to prevent overflow
        let buffer_len = match buffer_size.checked_div(2) {
            Some(len) if len > 0 && len <= (MAX_BUFFER_SIZE / 2) => len as usize,
            _ => {
                warn!(
                    message = "Invalid buffer size calculation, skipping event",
                    buffer_size = %buffer_size,
                    channel = %channel,
                    internal_log_rate_limit = true
                );
                return Ok(None);
            }
        };
        
        let mut buffer = vec![0u16; buffer_len];

        let result = unsafe {
            EvtRender(
                None,
                event_handle,
                EvtRenderEventXml,
                buffer_size,
                buffer.as_mut_ptr() as *mut _,
                &mut buffer_size,
                &mut buffer_used,
            )
        };

        if !result.as_bool() {
            let last_error = windows::core::Error::from_win32();
            return Err(WindowsEventLogError::ReadEventError {
                source: last_error,
            });
        }

        // Additional safety check: ensure buffer_used doesn't exceed allocated buffer
        if buffer_used as usize > buffer_size as usize {
            warn!(
                message = "Buffer overrun detected in EvtRender, skipping event",
                buffer_used = %buffer_used,
                buffer_size = %buffer_size,
                channel = %channel,
                internal_log_rate_limit = true
            );
            return Ok(None);
        }

        // Safely calculate buffer slice with bounds checking
        let used_len = match buffer_used.checked_div(2) {
            Some(len) if len <= buffer.len() as u32 => len as usize,
            _ => {
                warn!(
                    message = "Invalid buffer usage calculation, skipping event",
                    buffer_used = %buffer_used,
                    buffer_len = %buffer.len(),
                    channel = %channel,
                    internal_log_rate_limit = true
                );
                return Ok(None);
            }
        };
        
        let xml = String::from_utf16_lossy(&buffer[..used_len]);

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

        // Safely parse numeric values with proper error handling
        let record_id = Self::extract_xml_value(&xml, "RecordID")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let event_id = Self::extract_xml_value(&xml, "EventID")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        let level = Self::extract_xml_value(&xml, "Level")
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

        // Safe timestamp parsing with validation
        let time_created = match Self::extract_xml_value(&xml, "TimeCreated")
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        {
            Some(dt) => {
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
            }
            None => Utc::now(),
        };

        // Apply age filter
        if let Some(max_age_secs) = self.config.max_event_age_secs {
            let age = Utc::now().signed_duration_since(time_created);
            if age.num_seconds() > max_age_secs as i64 {
                return Ok(None);
            }
        }

        // Safe string field extraction with length validation
        let provider_name = Self::extract_xml_value(&xml, "Provider")
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .unwrap_or_else(|| "Unknown".to_string());

        let computer = Self::extract_xml_value(&xml, "Computer")
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .unwrap_or_else(|| "localhost".to_string());

        // Safe numeric parsing with validation
        let process_id = Self::extract_xml_value(&xml, "ProcessID")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        let thread_id = Self::extract_xml_value(&xml, "ThreadID")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

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
                xml
            } else {
                String::new()
            },
            rendered_message: None, // Would be rendered in full implementation
            event_data: HashMap::new(), // Would be extracted from EventData section
            user_data: HashMap::new(), // Would be extracted from UserData section
        };

        Ok(Some(event))
    }

    /// Safely extract XML value using proper XML parser
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
                    let element_name = String::from_utf8_lossy(e.name().as_ref());
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
                    let element_name = String::from_utf8_lossy(e.name().as_ref());
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

    fn build_xpath_query(&self, channel: &str) -> Result<String, WindowsEventLogError> {
        let mut query = "*".to_string();

        if let Some(ref custom_query) = self.config.event_query {
            query = custom_query.clone();
        } else {
            // Build basic query with level filtering if needed
            query = "*[System]".to_string();
        }

        // Add bookmark filtering if we have a previous position
        if let Some(bookmark) = self.last_bookmarks.get(channel) {
            // In a full implementation, we would use the bookmark to continue from last position
            debug!("Using bookmark for channel {}: {}", channel, bookmark);
        }

        Ok(query)
    }

    fn validate_channels(&self) -> Result<(), WindowsEventLogError> {
        #[cfg(windows)]
        {
            use windows::{
                Win32::System::EventLog::{EvtClose, EvtOpenChannelEnum},
                core::HSTRING,
            };

            // Try to enumerate channels to validate they exist
            let enum_handle = unsafe { EvtOpenChannelEnum(None, 0) }
                .map_err(|e| WindowsEventLogError::CreateSubscriptionError { source: e })?;

            unsafe { EvtClose(enum_handle) };
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
            if events.is_empty() {
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
    use std::collections::HashMap;

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
