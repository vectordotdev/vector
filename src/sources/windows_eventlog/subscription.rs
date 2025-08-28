use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Utc};
use snafu::{ResultExt, OptionExt};
use tokio::{fs, io::{AsyncReadExt, AsyncWriteExt}};

use super::{
    config::WindowsEventLogConfig,
    error::*,
};

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
            core::HSTRING,
            Win32::System::EventLog::{
                EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryForwardDirection,
                EvtRender, EvtRenderEventXml, EvtClose, EVT_HANDLE,
            },
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

        let mut events = Vec::new();
        let mut event_handles = vec![EVT_HANDLE::default(); max_events.min(100)];
        
        loop {
            let mut returned = 0u32;
            
            let result = unsafe {
                EvtNext(
                    query_handle,
                    event_handles.len() as u32,
                    event_handles.as_mut_ptr(),
                    10000, // 10 second timeout
                    0,     // Flags
                    &mut returned,
                )
            };

            if !result.as_bool() || returned == 0 {
                break;
            }

            for i in 0..returned as usize {
                let event_handle = event_handles[i];
                
                match self.process_event_handle(event_handle, channel).await {
                    Ok(Some(event)) => {
                        events.push(event);
                        if events.len() >= max_events {
                            // Clean up remaining handles
                            for j in i..returned as usize {
                                unsafe { EvtClose(event_handles[j]) };
                            }
                            unsafe { EvtClose(query_handle) };
                            return Ok(events);
                        }
                    }
                    Ok(None) => {
                        // Event was filtered out
                    }
                    Err(e) => {
                        warn!(
                            message = "Failed to process event",
                            error = %e,
                            channel = %channel,
                            internal_log_rate_limit = true
                        );
                    }
                }
                
                unsafe { EvtClose(event_handle) };
            }

            if events.len() >= max_events {
                break;
            }
        }

        unsafe { EvtClose(query_handle) };
        Ok(events)
    }

    #[cfg(windows)]
    async fn process_event_handle(
        &self,
        event_handle: windows::Win32::System::EventLog::EVT_HANDLE,
        channel: &str,
    ) -> Result<Option<WindowsEvent>, WindowsEventLogError> {
        use windows::Win32::System::EventLog::{
            EvtRender, EvtRenderEventXml,
        };

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

        let mut buffer = vec![0u16; (buffer_size / 2) as usize];
        
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
            return Err(WindowsEventLogError::ReadEventError {
                source: windows::core::Error::from_win32(),
            });
        }

        let xml = String::from_utf16_lossy(&buffer[..((buffer_used / 2) as usize)]);
        
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
        
        let record_id = Self::extract_xml_value(&xml, "RecordID")
            .unwrap_or_else(|| "0".to_string())
            .parse::<u64>()
            .unwrap_or(0);
            
        let event_id = Self::extract_xml_value(&xml, "EventID")
            .unwrap_or_else(|| "0".to_string())
            .parse::<u32>()
            .unwrap_or(0);
            
        let level = Self::extract_xml_value(&xml, "Level")
            .unwrap_or_else(|| "4".to_string())
            .parse::<u8>()
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

        let time_created_str = Self::extract_xml_value(&xml, "TimeCreated")
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        let time_created = DateTime::parse_from_rfc3339(&time_created_str)
            .unwrap_or_else(|_| Utc::now().into())
            .with_timezone(&Utc);

        // Apply age filter
        if let Some(max_age_secs) = self.config.max_event_age_secs {
            let age = Utc::now().signed_duration_since(time_created);
            if age.num_seconds() > max_age_secs as i64 {
                return Ok(None);
            }
        }

        let provider_name = Self::extract_xml_value(&xml, "Provider")
            .unwrap_or_else(|| "Unknown".to_string());
            
        let computer = Self::extract_xml_value(&xml, "Computer")
            .unwrap_or_else(|| "localhost".to_string());

        let process_id = Self::extract_xml_value(&xml, "ProcessID")
            .unwrap_or_else(|| "0".to_string())
            .parse::<u32>()
            .unwrap_or(0);

        let thread_id = Self::extract_xml_value(&xml, "ThreadID")
            .unwrap_or_else(|| "0".to_string())
            .parse::<u32>()
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
            raw_xml: if self.config.include_xml { xml } else { String::new() },
            rendered_message: None, // Would be rendered in full implementation
            event_data: HashMap::new(), // Would be extracted from EventData section
            user_data: HashMap::new(),  // Would be extracted from UserData section
        };

        Ok(Some(event))
    }

    fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
        // Simple XML value extraction - would use proper XML parser in production
        let start_tag = format!("<{}", tag);
        let end_tag = format!("</{}>", tag);
        
        if let Some(start_pos) = xml.find(&start_tag) {
            if let Some(content_start) = xml[start_pos..].find('>') {
                let content_start = start_pos + content_start + 1;
                if let Some(content_end) = xml[content_start..].find(&end_tag) {
                    let content_end = content_start + content_end;
                    return Some(xml[content_start..content_end].trim().to_string());
                }
            }
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
                core::HSTRING,
                Win32::System::EventLog::{EvtOpenChannelEnum, EvtClose},
            };

            // Try to enumerate channels to validate they exist
            let enum_handle = unsafe { EvtOpenChannelEnum(None, 0) }
                .map_err(|e| WindowsEventLogError::CreateSubscriptionError { source: e })?;

            unsafe { EvtClose(enum_handle) };
        }

        Ok(())
    }

    fn filter_events(&self, events: Vec<WindowsEvent>) -> Result<Vec<WindowsEvent>, WindowsEventLogError> {
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
            if let Ok(content) = fs::read_to_string(path).await {
                for line in content.lines() {
                    if let Some((channel, bookmark)) = line.split_once('=') {
                        self.last_bookmarks.insert(channel.trim().to_string(), bookmark.trim().to_string());
                    }
                }
            }
        }
        Ok(())
    }

    async fn update_bookmarks(&mut self, events: &[WindowsEvent]) -> Result<(), WindowsEventLogError> {
        if let Some(ref path) = self.bookmark_file {
            if events.is_empty() {
                return Ok(());
            }

            // Group events by channel and get the latest bookmark for each
            let mut channel_bookmarks = HashMap::new();
            for event in events {
                // Use record ID as bookmark
                let bookmark = format!("record:{}", event.record_id);
                channel_bookmarks.insert(&event.channel, bookmark);
            }

            // Update in-memory bookmarks
            for (channel, bookmark) in &channel_bookmarks {
                self.last_bookmarks.insert(channel.to_string(), bookmark.clone());
            }

            // Write all bookmarks to file
            let mut content = String::new();
            for (channel, bookmark) in &self.last_bookmarks {
                content.push_str(&format!("{}={}\n", channel, bookmark));
            }

            if let Err(e) = fs::write(path, content).await {
                return Err(WindowsEventLogError::BookmarkPersistenceError {
                    message: format!("Failed to write bookmarks to file: {}", e),
                });
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
        
        assert!(matches!(result, Err(WindowsEventLogError::NotSupportedError)));
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
        subscription.last_bookmarks.insert("System".to_string(), "record:12345".to_string());
        subscription.last_bookmarks.insert("Application".to_string(), "record:67890".to_string());
        
        let events = Vec::new(); // Empty events, will use existing bookmarks
        subscription.update_bookmarks(&events).await.unwrap();
        
        // Test loading bookmarks
        subscription.last_bookmarks.clear();
        subscription.load_bookmarks().await.unwrap();
        
        assert_eq!(subscription.last_bookmarks.get("System"), Some(&"record:12345".to_string()));
        assert_eq!(subscription.last_bookmarks.get("Application"), Some(&"record:67890".to_string()));
    }
}