use std::{
    collections::HashMap,
    num::NonZeroU32,
    sync::{Arc, Mutex, RwLock},
};

use chrono::{DateTime, Utc};
use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use quick_xml::{Reader, events::Event as XmlEvent};
use regex::{Regex, escape as regex_escape};
use tokio::sync::{mpsc, watch};

use super::{
    bookmark::BookmarkManager,
    checkpoint::Checkpointer,
    config::{WindowsEventLogConfig, is_channel_pattern},
    error::*,
};

use crate::internal_events::{WindowsEventLogBookmarkError, WindowsEventLogSubscriptionError};

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
    // Checkpointing is handled by CallbackContext via bookmarks
    #[cfg(windows)]
    #[allow(dead_code)] // Used for RAII cleanup of Windows handles via Drop trait
    subscriptions: Arc<Mutex<Vec<SubscriptionHandle>>>,
    #[cfg(windows)]
    // Shared subscription error state - checked by next_events()
    subscription_error: Arc<Mutex<Option<WindowsEventLogError>>>,
    #[cfg(windows)]
    // Callback context for accessing bookmarks/checkpointer on shutdown
    callback_context: Arc<CallbackContext>,
    // Rate limiter for controlling event throughput
    rate_limiter: Option<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    // Shutdown signal for background checkpoint task
    #[cfg(windows)]
    #[allow(dead_code)] // Sender is kept alive to signal shutdown on drop
    shutdown_tx: watch::Sender<bool>,
}

#[cfg(windows)]
struct SubscriptionHandle {
    handle: windows::Win32::System::EventLog::EVT_HANDLE,
    // Raw pointer to CallbackContext - must be freed in Drop to prevent memory leak
    context: *const std::ffi::c_void,
}

// SAFETY: SubscriptionHandle contains a raw pointer to Arc<CallbackContext>.
// Arc is thread-safe, and the raw pointer is only used for cleanup in Drop.
// The pointer is never dereferenced except to reconstruct the Arc when dropping.
#[cfg(windows)]
unsafe impl Send for SubscriptionHandle {}

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

        // Free the CallbackContext to prevent memory leak
        // EvtSubscribe incremented the Arc refcount via into_raw(), so we must decrement it
        if !self.context.is_null() {
            unsafe {
                let _ = Arc::from_raw(self.context as *const CallbackContext);
            }
        }
    }
}

#[cfg(windows)]
struct CallbackContext {
    event_sender: mpsc::UnboundedSender<WindowsEvent>,
    config: Arc<WindowsEventLogConfig>,
    // Shared error state - allows callback to signal fatal subscription errors
    subscription_error: Arc<Mutex<Option<WindowsEventLogError>>>,
    // Per-channel bookmarks for checkpoint tracking (RwLock for better concurrency)
    // - Background task takes read lock to serialize bookmarks
    // - Callback takes write lock briefly to update bookmarks
    bookmarks: Arc<RwLock<HashMap<String, BookmarkManager>>>,
    // Channels that have valid checkpoints (restored from disk)
    // Used to distinguish fresh bookmarks from restored ones for read_existing_events logic
    channels_with_checkpoints: std::collections::HashSet<String>,
    // Checkpointer for periodic bookmark persistence
    checkpointer: Arc<Checkpointer>,
}

// Convert CallbackContext pointer to raw pointer for passing through Windows API
#[cfg(windows)]
impl CallbackContext {
    fn into_raw(ctx: Arc<CallbackContext>) -> *const std::ffi::c_void {
        Arc::into_raw(ctx) as *const std::ffi::c_void
    }

    unsafe fn from_raw(ptr: *const std::ffi::c_void) -> Arc<CallbackContext> {
        unsafe { Arc::from_raw(ptr as *const CallbackContext) }
    }

    /// Get reference to checkpointer (ensures field is marked as used)
    fn checkpointer(&self) -> &Arc<Checkpointer> {
        &self.checkpointer
    }
}

impl EventLogSubscription {
    /// Create a new event-driven subscription using EvtSubscribe with callback
    pub async fn new(
        config: &WindowsEventLogConfig,
        checkpointer: Arc<Checkpointer>,
    ) -> Result<Self, WindowsEventLogError> {
        // Create rate limiter if configured
        let rate_limiter = if config.events_per_second > 0 {
            NonZeroU32::new(config.events_per_second).map(|rate| {
                info!(
                    message = "Enabling rate limiting for Windows Event Log source",
                    events_per_second = config.events_per_second
                );
                RateLimiter::direct(Quota::per_second(rate))
            })
        } else {
            None
        };

        #[cfg(not(windows))]
        {
            let _ = checkpointer; // Suppress unused warning on non-Windows
            return Err(WindowsEventLogError::NotSupportedError);
        }

        #[cfg(windows)]
        {
            // Expand channel patterns (e.g., "Microsoft-Windows-*") to actual channel names
            let expanded_channels = expand_channel_patterns(&config.channels)?;

            // Create config with expanded channels
            let mut expanded_config = config.clone();
            expanded_config.channels = expanded_channels;
            let config = Arc::new(expanded_config);

            let (event_sender, event_receiver) = mpsc::unbounded_channel();
            let subscription_error = Arc::new(Mutex::new(None));

            // Validate channels exist and are accessible
            Self::validate_channels(&config)?;

            // Initialize bookmarks from checkpoints or create fresh ones
            // Track which channels have valid checkpoints (for read_existing_events logic)
            let mut bookmarks = HashMap::new();
            let mut channels_with_checkpoints = std::collections::HashSet::new();
            for channel in &config.channels {
                let bookmark = if let Some(checkpoint) = checkpointer.get(channel).await {
                    info!(
                        message = "Resuming from checkpoint bookmark",
                        channel = %channel
                    );
                    channels_with_checkpoints.insert(channel.clone());
                    BookmarkManager::from_xml(&checkpoint.bookmark_xml)?
                } else {
                    info!(
                        message = "No checkpoint found, creating fresh bookmark",
                        channel = %channel
                    );
                    BookmarkManager::new()?
                };
                bookmarks.insert(channel.clone(), bookmark);
            }

            // Create callback context for this subscription
            let callback_context = Arc::new(CallbackContext {
                event_sender: event_sender.clone(),
                config: Arc::clone(&config),
                subscription_error: Arc::clone(&subscription_error),
                bookmarks: Arc::new(RwLock::new(bookmarks)),
                channels_with_checkpoints,
                checkpointer: Arc::clone(&checkpointer),
            });

            let subscriptions = Arc::new(Mutex::new(Vec::new()));

            // Create shutdown channel for background task
            let (shutdown_tx, shutdown_rx) = watch::channel(false);

            // Create subscriptions for each channel, passing our context
            Self::create_subscriptions(
                &config,
                Arc::clone(&subscriptions),
                Arc::clone(&callback_context),
            )?;

            // Start background task to periodically save bookmarks (every 5 seconds)
            // This avoids blocking event processing with serialization/I/O
            let checkpoint_saver_ctx = Arc::clone(&callback_context);
            let mut shutdown_rx_clone = shutdown_rx.clone();
            tokio::spawn(async move {
                debug!(message = "Checkpoint saver background task started");
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
                loop {
                    // Wait for either interval tick or shutdown signal
                    tokio::select! {
                        _ = interval.tick() => {}
                        _ = shutdown_rx_clone.changed() => {
                            debug!(message = "Checkpoint saver task received shutdown signal");
                            break;
                        }
                    }

                    // Step 1: Copy handles while holding lock briefly (handles are just integers)
                    let bookmark_handles: Vec<(
                        String,
                        windows::Win32::System::EventLog::EVT_HANDLE,
                    )> = {
                        let bookmarks = match checkpoint_saver_ctx.bookmarks.read() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                warn!(message = "Bookmark lock poisoned, recovering");
                                poisoned.into_inner()
                            }
                        };
                        bookmarks
                            .iter()
                            .map(|(channel, bookmark)| (channel.clone(), bookmark.as_handle()))
                            .collect()
                    }; // Lock released IMMEDIATELY

                    // Step 2: Serialize handles OUTSIDE the lock (slow Windows API calls)
                    let bookmark_xmls: Vec<(String, String)> = bookmark_handles
                        .into_iter()
                        .filter_map(|(channel, handle)| {
                            match BookmarkManager::serialize_handle(handle) {
                                Ok(xml) if Self::is_valid_bookmark_xml(&xml) => {
                                    Some((channel, xml))
                                }
                                Ok(_) => None, // Empty or invalid = bookmark not yet updated with events
                                Err(e) => {
                                    emit!(WindowsEventLogBookmarkError {
                                        channel: channel.clone(),
                                        error: e.to_string(),
                                    });
                                    None
                                }
                            }
                        })
                        .collect();

                    // Save all checkpoints in a single batched disk write (no locks held)
                    if !bookmark_xmls.is_empty() {
                        if let Err(e) = checkpoint_saver_ctx
                            .checkpointer()
                            .set_batch(bookmark_xmls)
                            .await
                        {
                            error!(
                                message = "Failed to save bookmark checkpoints",
                                error = %e
                            );
                        }
                    }
                }
                debug!(message = "Checkpoint saver task terminated");
            });

            Ok(Self {
                config,
                event_receiver,
                subscriptions,
                subscription_error,
                callback_context,
                rate_limiter,
                shutdown_tx,
            })
        }
    }

    /// Flush all bookmarks to checkpoint storage
    ///
    /// Call this before shutdown to ensure no events are lost.
    /// Saves all current bookmarks for all channels to disk.
    #[cfg(windows)]
    pub async fn flush_bookmarks(&self) -> Result<(), WindowsEventLogError> {
        debug!(message = "Flushing bookmarks to checkpoint storage");

        // Signal shutdown to background task first
        let _ = self.shutdown_tx.send(true);

        // Copy handles quickly while holding lock, then serialize outside
        let bookmark_handles: Vec<(String, windows::Win32::System::EventLog::EVT_HANDLE)> = {
            let bookmarks = match self.callback_context.bookmarks.read() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!(message = "Bookmark lock poisoned during flush, recovering");
                    poisoned.into_inner()
                }
            };
            bookmarks
                .iter()
                .map(|(channel, bookmark)| (channel.clone(), bookmark.as_handle()))
                .collect()
        };
        // Lock released immediately

        // Serialize outside the lock
        let bookmark_xmls: Vec<(String, String)> = bookmark_handles
            .into_iter()
            .filter_map(|(channel, handle)| {
                match BookmarkManager::serialize_handle(handle) {
                    Ok(xml) if Self::is_valid_bookmark_xml(&xml) => Some((channel, xml)),
                    Ok(_) => None, // Empty or invalid = bookmark not yet updated
                    Err(e) => {
                        emit!(WindowsEventLogBookmarkError {
                            channel: channel.clone(),
                            error: e.to_string(),
                        });
                        None
                    }
                }
            })
            .collect();

        // Save all bookmarks in a single batched write
        if !bookmark_xmls.is_empty() {
            if let Err(e) = self
                .callback_context
                .checkpointer()
                .set_batch(bookmark_xmls)
                .await
            {
                warn!(
                    message = "Failed to flush bookmarks on shutdown",
                    error = %e
                );
            }
        }

        debug!(message = "Bookmark flush complete");
        Ok(())
    }

    /// Get the current bookmark XML for a specific channel.
    ///
    /// This is used for acknowledgment-based checkpointing where the bookmark
    /// state needs to be captured when events are read (not when they're acknowledged).
    /// Returns None if the channel has no valid bookmark yet.
    #[cfg(windows)]
    pub fn get_bookmark_xml(&self, channel: &str) -> Option<String> {
        let bookmarks = match self.callback_context.bookmarks.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        bookmarks.get(channel).and_then(|bookmark| {
            let handle = bookmark.as_handle();
            match BookmarkManager::serialize_handle(handle) {
                Ok(xml) if Self::is_valid_bookmark_xml(&xml) => Some(xml),
                _ => None,
            }
        })
    }

    /// Get the current bookmark XML for a specific channel (non-Windows stub).
    #[cfg(not(windows))]
    pub fn get_bookmark_xml(&self, _channel: &str) -> Option<String> {
        None
    }

    /// Get the next batch of events from the subscription
    pub async fn next_events(
        &mut self,
        max_events: usize,
    ) -> Result<Vec<WindowsEvent>, WindowsEventLogError> {
        use tokio::time::{Duration, timeout};

        // Check for subscription errors first
        #[cfg(windows)]
        {
            if let Some(error) = self.subscription_error.lock().unwrap().take() {
                return Err(error);
            }
        }

        let mut events = Vec::with_capacity(max_events.min(1000));

        // Use timeout to prevent blocking indefinitely
        let timeout_duration = Duration::from_millis(self.config.event_timeout_ms);

        while events.len() < max_events {
            // Check for subscription errors during event collection
            #[cfg(windows)]
            {
                if let Some(error) = self.subscription_error.lock().unwrap().take() {
                    return Err(error);
                }
            }

            match timeout(timeout_duration, self.event_receiver.recv()).await {
                Ok(Some(event)) => {
                    // Bookmarks handle resume/deduplication automatically via EvtSubscribe
                    // Event ID and age filtering already applied in parse_event_xml
                    // Field filtering is applied in the parser
                    // Apply rate limiting before adding the event
                    if let Some(limiter) = &self.rate_limiter {
                        limiter.until_ready().await;
                    }
                    events.push(event);
                }
                Ok(None) => {
                    // Channel closed, subscription ended
                    debug!("Event subscription channel closed");

                    // Check if closure was due to an error
                    #[cfg(windows)]
                    {
                        if let Some(error) = self.subscription_error.lock().unwrap().take() {
                            return Err(error);
                        }
                    }
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

        // Bookmarks are automatically updated per-event in the callback
        // and periodically saved to checkpoints, so no batch-level checkpointing needed
        Ok(events)
    }

    #[cfg(windows)]
    fn create_subscriptions(
        config: &Arc<WindowsEventLogConfig>,
        subscriptions: Arc<Mutex<Vec<SubscriptionHandle>>>,
        callback_context: Arc<CallbackContext>,
    ) -> Result<(), WindowsEventLogError> {
        use windows::{
            Win32::System::EventLog::{
                EVT_HANDLE, EvtSubscribe, EvtSubscribeStartAfterBookmark,
                EvtSubscribeStartAtOldestRecord, EvtSubscribeToFutureEvents,
            },
            core::HSTRING,
        };

        info!("Creating Windows Event Log subscriptions");

        for channel in &config.channels {
            let channel_hstring = HSTRING::from(channel.as_str());
            let query = Self::build_xpath_query(config)?;
            let query_hstring = HSTRING::from(query.clone());

            // Check if this channel has a valid checkpoint (restored from disk)
            // This is different from just having a bookmark object (we always create one)
            let has_valid_checkpoint = callback_context.channels_with_checkpoints.contains(channel);

            // Get bookmark handle for this channel from context
            let bookmark_handle = {
                let bookmarks = match callback_context.bookmarks.read() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                bookmarks
                    .get(channel)
                    .map(|bm| bm.as_handle())
                    .unwrap_or(EVT_HANDLE::default())
            };

            // Determine subscription flags:
            // - If we have a valid checkpoint, resume from bookmark
            // - If no checkpoint AND read_existing_events=true, start from oldest
            // - If no checkpoint AND read_existing_events=false, start from future events only
            let subscription_flags = if has_valid_checkpoint {
                EvtSubscribeStartAfterBookmark.0
            } else if config.read_existing_events {
                EvtSubscribeStartAtOldestRecord.0
            } else {
                EvtSubscribeToFutureEvents.0
            };

            debug!(
                message = "Creating Windows Event Log subscription",
                channel = %channel,
                query = %query,
                has_valid_checkpoint = has_valid_checkpoint,
                read_existing = config.read_existing_events
            );

            // Convert context to raw pointer for passing through Windows API
            let context_ptr = CallbackContext::into_raw(Arc::clone(&callback_context));

            // Create subscription using EvtSubscribe with callback and bookmark
            let subscription_result = unsafe {
                if has_valid_checkpoint {
                    EvtSubscribe(
                        None, // Session handle (local)
                        None, // Signal event (we use callback instead)
                        &channel_hstring,
                        &query_hstring,
                        bookmark_handle,   // Bookmark for resume from checkpoint
                        Some(context_ptr), // Context - per-subscription context!
                        Some(event_subscription_callback), // Callback function
                        subscription_flags,
                    )
                } else {
                    EvtSubscribe(
                        None, // Session handle (local)
                        None, // Signal event (we use callback instead)
                        &channel_hstring,
                        &query_hstring,
                        None,                              // No bookmark for fresh start
                        Some(context_ptr),                 // Context - per-subscription context!
                        Some(event_subscription_callback), // Callback function
                        subscription_flags,
                    )
                }
            };

            match subscription_result {
                Ok(subscription_handle) => {
                    info!(
                        message = "Windows Event Log subscription created successfully",
                        channel = %channel
                    );

                    // Store subscription handle for cleanup
                    {
                        let mut subs = subscriptions.lock().unwrap();
                        subs.push(SubscriptionHandle {
                            handle: subscription_handle,
                            context: context_ptr,
                        });
                    }
                }
                Err(e) => {
                    // ERROR_EVT_CHANNEL_CANNOT_ACTIVATE (0x80073AA1) means this is a
                    // direct/analytic channel that can't be subscribed to - skip it
                    // Also handle access denied gracefully for wildcard expansions
                    let error_code = e.code().0 as u32;
                    if error_code == 0x80073AA1 {
                        warn!(
                            message = "Skipping direct/analytic channel (cannot subscribe)",
                            channel = %channel
                        );
                        // Clean up the context pointer since we won't use it
                        unsafe {
                            let _ = CallbackContext::from_raw(context_ptr);
                        }
                        continue;
                    } else if error_code == 5 {
                        // Access denied - skip with warning
                        warn!(
                            message = "Skipping channel due to access denied",
                            channel = %channel
                        );
                        unsafe {
                            let _ = CallbackContext::from_raw(context_ptr);
                        }
                        continue;
                    } else {
                        // Other errors are fatal
                        return Err(WindowsEventLogError::CreateSubscriptionError { source: e });
                    }
                }
            }
        }

        // Verify we subscribed to at least one channel
        let sub_count = subscriptions.lock().unwrap().len();
        if sub_count == 0 {
            return Err(WindowsEventLogError::ConfigError {
                message: "No channels could be subscribed to. All channels may be inaccessible or direct/analytic channels.".into(),
            });
        }

        info!(
            message = "Successfully subscribed to channels",
            channel_count = sub_count
        );

        Ok(())
    }

    fn build_xpath_query(config: &WindowsEventLogConfig) -> Result<String, WindowsEventLogError> {
        let query = if let Some(ref custom_query) = config.event_query {
            custom_query.clone()
        } else {
            "*".to_string()
        };

        Ok(query)
    }

    #[cfg(windows)]
    fn validate_channels(config: &WindowsEventLogConfig) -> Result<(), WindowsEventLogError> {
        use windows::Win32::System::EventLog::{EvtClose, EvtOpenChannelConfig};
        use windows::core::HSTRING;

        // Validate each channel exists and is accessible
        for channel in &config.channels {
            let channel_hstring = HSTRING::from(channel.as_str());

            // Try to open the channel configuration to verify it exists
            let channel_handle = unsafe { EvtOpenChannelConfig(None, &channel_hstring, 0) };

            match channel_handle {
                Ok(handle) => {
                    // Channel exists - close the handle and continue
                    if let Err(e) = unsafe { EvtClose(handle) } {
                        warn!("Failed to close channel config handle: {}", e);
                    }
                }
                Err(e) => {
                    // Channel doesn't exist or can't be accessed
                    let error_code = e.code().0 as u32;

                    // ERROR_FILE_NOT_FOUND (2) or ERROR_EVT_CHANNEL_NOT_FOUND (15007)
                    if error_code == 2 || error_code == 15007 {
                        return Err(WindowsEventLogError::ChannelNotFoundError {
                            channel: channel.clone(),
                        });
                    } else if error_code == 5 {
                        // ERROR_ACCESS_DENIED
                        return Err(WindowsEventLogError::AccessDeniedError {
                            channel: channel.clone(),
                        });
                    } else {
                        return Err(WindowsEventLogError::OpenChannelError {
                            channel: channel.clone(),
                            source: e,
                        });
                    }
                }
            }
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
            version: Self::extract_xml_value(xml, "Version").and_then(|v| v.parse().ok()),
            qualifiers: Self::extract_xml_attribute(xml, "Qualifiers").and_then(|v| v.parse().ok()),
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
            provider_name: Self::extract_provider_name(xml).unwrap_or_default(),
            provider_guid: Self::extract_xml_attribute(xml, "Guid"),
        }
    }

    /// Check if bookmark XML is valid (contains an actual bookmark position)
    ///
    /// Windows may return XML structure for empty bookmarks like `<BookmarkList/>`
    /// which we should NOT save as a checkpoint. A valid bookmark must contain
    /// a `<Bookmark` element with `RecordId` attribute.
    fn is_valid_bookmark_xml(xml: &str) -> bool {
        // Must be non-empty and contain actual bookmark data
        // Valid bookmark looks like: <BookmarkList><Bookmark Channel='...' RecordId='123' .../>
        !xml.is_empty() && xml.contains("<Bookmark") && xml.contains("RecordId")
    }

    // XML parsing helper methods - cleaned up and more secure
    pub fn extract_xml_attribute(xml: &str, attr_name: &str) -> Option<String> {
        // Use regex with proper escaping to prevent injection
        let pattern = format!(r#"{}="([^"]+)""#, regex_escape(attr_name));
        Regex::new(&pattern)
            .ok()?
            .captures(xml)?
            .get(1)
            .map(|m| m.as_str().to_string())
    }

    /// Extract provider name specifically from Provider element using proper XML parsing
    fn extract_provider_name(xml: &str) -> Option<String> {
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);
        let mut buf = Vec::new();

        // Limit iterations for security
        const MAX_ITERATIONS: usize = 1000;
        let mut iterations = 0;

        loop {
            if iterations >= MAX_ITERATIONS {
                return None;
            }
            iterations += 1;

            match reader.read_event_into(&mut buf) {
                Ok(XmlEvent::Start(ref e)) | Ok(XmlEvent::Empty(ref e)) => {
                    let name = e.name();
                    // Check if this is a Provider element (local name only, ignore namespace)
                    if name.local_name().as_ref() == b"Provider" {
                        // Extract the Name attribute
                        for attr in e.attributes() {
                            if let Ok(attr) = attr {
                                if attr.key.local_name().as_ref() == b"Name" {
                                    return String::from_utf8(attr.value.to_vec()).ok();
                                }
                            }
                        }
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
    pub fn extract_event_data(xml: &str, config: &WindowsEventLogConfig) -> EventDataResult {
        let mut structured_data = HashMap::new();
        let mut string_inserts = Vec::new();
        let mut user_data = HashMap::new();

        Self::parse_section(xml, "EventData", &mut structured_data, &mut string_inserts);
        Self::parse_section(xml, "UserData", &mut user_data, &mut Vec::new());

        // Apply configurable truncation to event data values
        if config.max_event_data_length > 0 {
            for value in structured_data.values_mut() {
                if value.len() > config.max_event_data_length {
                    value.truncate(config.max_event_data_length);
                    value.push_str("...[truncated]");
                }
            }
            for value in user_data.values_mut() {
                if value.len() > config.max_event_data_length {
                    value.truncate(config.max_event_data_length);
                    value.push_str("...[truncated]");
                }
            }
            for value in string_inserts.iter_mut() {
                if value.len() > config.max_event_data_length {
                    value.truncate(config.max_event_data_length);
                    value.push_str("...[truncated]");
                }
            }
        }

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
        inserts: &mut Vec<String>,
    ) {
        let mut reader = Reader::from_str(xml);
        reader.trim_text(true);

        let mut buf = Vec::new();
        let mut inside_section = false;
        let mut inside_data = false;
        let mut current_data_name = String::new();
        let mut current_data_value = String::new();

        const MAX_ITERATIONS: usize = 500; // Security limit
        const MAX_FIELDS: usize = 100; // Memory limit
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

                        // Note: Truncation is now configurable and handled later
                        // Store in appropriate format based on whether Name attribute exists
                        if !current_data_name.is_empty() {
                            named_data
                                .insert(current_data_name.clone(), current_data_value.clone());
                        } else if section_name == "EventData" {
                            // Add to StringInserts for FluentBit compatibility
                            inserts.push(current_data_value.clone());
                        }
                    }
                }
                Ok(XmlEvent::Text(ref e)) => {
                    if inside_section && inside_data {
                        if let Ok(text) = e.unescape() {
                            // Append text without length check (configurable truncation applied later)
                            // Still enforce a maximum for security to prevent OOM
                            const MAX_VALUE_SIZE: usize = 1024 * 1024; // 1MB hard limit for security
                            if current_data_value.len() + text.len() <= MAX_VALUE_SIZE {
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

    fn extract_message_from_xml(
        xml: &str,
        event_id: u32,
        provider_name: &str,
        computer: &str,
        config: &WindowsEventLogConfig,
    ) -> Option<String> {
        let event_data_result = Self::extract_event_data(xml, config);
        let event_data = &event_data_result.structured_data;

        // Helper function to apply configurable truncation
        let truncate = |s: &str| -> String {
            if config.max_message_field_length > 0 && s.len() > config.max_message_field_length {
                let mut truncated = s
                    .chars()
                    .take(config.max_message_field_length)
                    .collect::<String>();
                truncated.push_str("...");
                truncated
            } else {
                s.to_string()
            }
        };

        match event_id {
            6009 => {
                if let (Some(version), Some(build)) =
                    (event_data.get("Data_0"), event_data.get("Data_1"))
                {
                    return Some(format!(
                        "Microsoft Windows kernel version {} build {} started",
                        truncate(version),
                        truncate(build)
                    ));
                }
            }
            _ => {
                if !event_data.is_empty() {
                    let data_summary: Vec<String> = event_data
                        .iter()
                        .take(3)
                        .map(|(k, v)| format!("{}={}", truncate(k), truncate(v)))
                        .collect();
                    if !data_summary.is_empty() {
                        return Some(format!(
                            "Event ID {} from {} ({})",
                            event_id,
                            truncate(provider_name),
                            data_summary.join(", ")
                        ));
                    }
                }
            }
        }

        Some(format!(
            "Event ID {} from {} on {}",
            event_id,
            truncate(provider_name),
            truncate(computer)
        ))
    }

    fn parse_event_xml(
        xml: String,
        channel: &str,
        config: &WindowsEventLogConfig,
        pre_rendered_message: Option<String>,
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
                if diff > 365 * 10 { now } else { dt_utc }
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
        let event_data_result = Self::extract_event_data(&xml, config);

        // Use pre-rendered message from EvtFormatMessage if available,
        // otherwise fall back to XML-based extraction
        let rendered_message = pre_rendered_message.or_else(|| {
            Self::extract_message_from_xml(
                &xml,
                system_fields.event_id,
                &system_fields.provider_name,
                &system_fields.computer,
                config,
            )
        });

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

// CallbackContext cleanup is handled in SubscriptionHandle::drop via Arc::from_raw

/// Enumerate all available Windows Event Log channels on the system
#[cfg(windows)]
fn enumerate_all_channels() -> Result<Vec<String>, WindowsEventLogError> {
    use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS};
    use windows::Win32::System::EventLog::{EvtClose, EvtNextChannelPath, EvtOpenChannelEnum};

    let enum_handle = unsafe {
        EvtOpenChannelEnum(None, 0)
            .map_err(|e| WindowsEventLogError::ChannelEnumerationError { source: e })?
    };

    let mut channels = Vec::new();
    let mut buffer = vec![0u16; 512]; // Most channel names are < 256 chars

    loop {
        let mut buffer_used: u32 = 0;

        let result =
            unsafe { EvtNextChannelPath(enum_handle, Some(&mut buffer), &mut buffer_used) };

        match result {
            Ok(()) => {
                // Convert UTF-16 to String
                let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
                let channel = String::from_utf16_lossy(&buffer[..len]);
                if !channel.is_empty() {
                    channels.push(channel);
                }
            }
            Err(e) => {
                // ERROR_NO_MORE_ITEMS means we've enumerated all channels
                if e.code() == ERROR_NO_MORE_ITEMS.into() {
                    break;
                }
                // ERROR_INSUFFICIENT_BUFFER - resize and retry
                if e.code() == ERROR_INSUFFICIENT_BUFFER.into() && buffer_used > 0 {
                    buffer.resize(buffer_used as usize, 0);
                    continue;
                }
                // Other errors - close handle and return error
                unsafe {
                    let _ = EvtClose(enum_handle);
                }
                return Err(WindowsEventLogError::ChannelEnumerationError { source: e });
            }
        }
    }

    unsafe {
        let _ = EvtClose(enum_handle);
    }

    debug!(
        message = "Enumerated Windows Event Log channels",
        channel_count = channels.len()
    );

    Ok(channels)
}

/// Expand channel patterns (e.g., "Microsoft-Windows-*") to actual channel names
#[cfg(windows)]
fn expand_channel_patterns(patterns: &[String]) -> Result<Vec<String>, WindowsEventLogError> {
    use glob::Pattern;

    let mut matched_channels = Vec::new();
    let mut has_patterns = false;

    // Check if any patterns need expansion
    for pattern in patterns {
        if is_channel_pattern(pattern) {
            has_patterns = true;
            break;
        }
    }

    // If no patterns, return the original list (will be validated later)
    if !has_patterns {
        return Ok(patterns.to_vec());
    }

    // Enumerate all channels for pattern matching
    let all_channels = enumerate_all_channels()?;

    for pattern_str in patterns {
        if is_channel_pattern(pattern_str) {
            // Compile glob pattern (case-insensitive for Windows)
            let pattern =
                Pattern::new(pattern_str).map_err(|e| WindowsEventLogError::ConfigError {
                    message: format!("Invalid channel pattern '{}': {}", pattern_str, e),
                })?;

            let mut pattern_matched = false;
            for channel in &all_channels {
                // Case-insensitive matching
                if pattern.matches(&channel.to_lowercase()) || pattern.matches(channel) {
                    matched_channels.push(channel.clone());
                    pattern_matched = true;
                }
            }

            if !pattern_matched {
                warn!(
                    message = "Channel pattern matched no channels",
                    pattern = %pattern_str
                );
            }
        } else {
            // Exact channel name - include as-is (will be validated later)
            matched_channels.push(pattern_str.clone());
        }
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    matched_channels.retain(|c| seen.insert(c.clone()));

    if matched_channels.is_empty() {
        return Err(WindowsEventLogError::ConfigError {
            message: "No channels matched any of the specified patterns".into(),
        });
    }

    info!(
        message = "Expanded channel patterns",
        patterns = ?patterns,
        matched_channels = matched_channels.len()
    );

    Ok(matched_channels)
}

/// Non-Windows stub for pattern expansion
#[cfg(not(windows))]
fn expand_channel_patterns(patterns: &[String]) -> Result<Vec<String>, WindowsEventLogError> {
    // On non-Windows, just return patterns as-is (they'll fail at subscription time anyway)
    Ok(patterns.to_vec())
}

/// Format event message using Windows EvtFormatMessage API
///
/// This provides properly localized, parameter-substituted messages like
/// "An account was successfully logged on" instead of raw event data.
/// Returns None on any failure (graceful fallback to XML-based extraction).
#[cfg(windows)]
fn format_event_message(
    event_handle: windows::Win32::System::EventLog::EVT_HANDLE,
    provider_name: &str,
) -> Option<String> {
    use windows::Win32::System::EventLog::{
        EvtClose, EvtFormatMessage, EvtFormatMessageEvent, EvtOpenPublisherMetadata,
    };
    use windows::core::HSTRING;

    const MAX_MESSAGE_SIZE: usize = 64 * 1024; // 64KB max for messages

    // Open publisher metadata to get message templates
    let provider_hstring = HSTRING::from(provider_name);
    let metadata_handle = unsafe {
        match EvtOpenPublisherMetadata(None, &provider_hstring, None, 0, 0) {
            Ok(handle) => handle,
            Err(_) => return None, // Provider not found - graceful fallback
        }
    };

    // Two-pass buffer allocation for EvtFormatMessage
    // First call with None buffer to get required size
    let mut buffer_used: u32 = 0;
    let _ = unsafe {
        EvtFormatMessage(
            metadata_handle,
            event_handle,
            0,
            None,
            EvtFormatMessageEvent.0,
            None,
            &mut buffer_used,
        )
    };

    // Check if we got a valid size
    if buffer_used == 0 || buffer_used as usize > MAX_MESSAGE_SIZE {
        unsafe {
            let _ = EvtClose(metadata_handle);
        }
        return None;
    }

    // Allocate buffer and get the formatted message
    let mut buffer = vec![0u16; buffer_used as usize];
    let mut actual_used: u32 = 0;

    let result = unsafe {
        EvtFormatMessage(
            metadata_handle,
            event_handle,
            0,
            None,
            EvtFormatMessageEvent.0,
            Some(&mut buffer),
            &mut actual_used,
        )
    };

    // Close metadata handle regardless of result
    unsafe {
        let _ = EvtClose(metadata_handle);
    }

    if result.is_err() {
        return None;
    }

    // Convert UTF-16 to String, trimming null terminator
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let message = String::from_utf16_lossy(&buffer[..len]);

    if message.is_empty() {
        None
    } else {
        Some(message)
    }
}

// Windows Event Log subscription callback function
#[cfg(windows)]
unsafe extern "system" fn event_subscription_callback(
    action: windows::Win32::System::EventLog::EVT_SUBSCRIBE_NOTIFY_ACTION,
    user_context: *const std::ffi::c_void,
    event_handle: windows::Win32::System::EventLog::EVT_HANDLE,
) -> u32 {
    use windows::Win32::System::EventLog::{EvtSubscribeActionDeliver, EvtSubscribeActionError};

    // Safety check: user_context must not be null
    if user_context.is_null() {
        error!("Callback received null user_context - cancelling subscription");
        return windows::Win32::Foundation::ERROR_CANCELLED.0;
    }

    // Retrieve the callback context from user_context parameter
    // Clone the Arc to increment ref count, then immediately forget it to not drop
    let ctx = unsafe {
        let arc = CallbackContext::from_raw(user_context);
        let cloned = Arc::clone(&arc);
        std::mem::forget(arc); // Don't drop - Windows still owns this
        cloned
    };

    #[allow(non_upper_case_globals)] // Windows API constants don't follow Rust conventions
    match action {
        EvtSubscribeActionDeliver => {
            // Process the event with the correct context
            if let Err(e) = process_callback_event(event_handle, &ctx) {
                warn!("Error processing callback event: {}", e);
            }
            0 // Return success
        }
        EvtSubscribeActionError => {
            // Extract the actual Windows error using EvtGetExtendedStatus
            let error_message = unsafe { extract_windows_extended_status() };

            // Emit internal event for metrics
            emit!(WindowsEventLogSubscriptionError {
                error: error_message.clone(),
                channels: ctx.config.channels.clone(),
            });

            // Store the error in the shared state so next_events() can retrieve it
            let mut error_state = ctx.subscription_error.lock().unwrap();
            if error_state.is_none() {
                *error_state = Some(WindowsEventLogError::SubscriptionError {
                    source: windows::core::Error::from_win32(),
                });
            }

            // Return ERROR_CANCELLED to signal Windows that we're done with this subscription
            // This prevents the callback storm by telling Windows to stop calling us
            windows::Win32::Foundation::ERROR_CANCELLED.0
        }
        _ => {
            debug!("Unknown subscription callback action: {}", action.0);
            0
        }
    }
}

/// Extract extended error information from Windows Event Log API
#[cfg(windows)]
unsafe fn extract_windows_extended_status() -> String {
    use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows::Win32::System::EventLog::EvtGetExtendedStatus;

    const MAX_ERROR_BUFFER: usize = 8192; // 8KB max for error messages
    let mut buffer_size = 0u32;

    // First call to get required buffer size
    let status = unsafe { EvtGetExtendedStatus(None, &mut buffer_size) };

    if status != ERROR_INSUFFICIENT_BUFFER.0 || buffer_size == 0 {
        return "Unknown error (unable to retrieve extended status)".to_string();
    }

    // Enforce maximum size but still try to get the error message
    if buffer_size as usize > MAX_ERROR_BUFFER {
        warn!(
            "Extended error status buffer size {} exceeds maximum {}, truncating",
            buffer_size, MAX_ERROR_BUFFER
        );
        buffer_size = MAX_ERROR_BUFFER as u32;
    }

    // Allocate buffer and retrieve the error message
    let mut buffer = vec![0u16; buffer_size as usize];
    let mut actual_size = 0u32;

    let status = unsafe { EvtGetExtendedStatus(Some(&mut buffer), &mut actual_size) };

    if status == 0 {
        // Success - remove null terminator if present
        let msg_len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..msg_len])
    } else {
        "Unknown error (EvtGetExtendedStatus failed)".to_string()
    }
}

/// Update bookmark for a channel (in-memory only, fast)
///
/// The callback should ONLY update bookmarks, not serialize/save them.
/// Serialization happens in a background task to avoid blocking event processing.
///
/// Uses RwLock for better concurrency - write lock is held only briefly for
/// the in-memory update (< 1 microsecond), allowing concurrent reads during
/// checkpoint serialization.
#[cfg(windows)]
fn update_bookmark_and_checkpoint(
    ctx: &Arc<CallbackContext>,
    channel: String,
    event_handle: windows::Win32::System::EventLog::EVT_HANDLE,
) {
    // ONLY update the bookmark in-memory (< 1 microsecond, no I/O)
    // Do NOT serialize or save here - that happens in background task
    let mut bookmarks = match ctx.bookmarks.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!(message = "Bookmark lock poisoned during update, recovering");
            poisoned.into_inner()
        }
    };
    if let Some(bookmark) = bookmarks.get_mut(&channel) {
        if let Err(e) = bookmark.update(event_handle) {
            emit!(WindowsEventLogBookmarkError {
                channel: channel.clone(),
                error: e.to_string(),
            });
        }
    }
    // Lock released - event processing continues immediately
}

#[cfg(windows)]
fn process_callback_event(
    event_handle: windows::Win32::System::EventLog::EVT_HANDLE,
    ctx: &Arc<CallbackContext>,
) -> Result<(), WindowsEventLogError> {
    use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
    use windows::Win32::System::EventLog::{EvtRender, EvtRenderEventXml};

    const MAX_BUFFER_SIZE: u32 = 10 * 1024 * 1024; // 10MB limit (increased to handle large events)
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
            if buffer_used == 0 {
                warn!("Event XML buffer size is zero, skipping event");
                return Ok(());
            }

            if buffer_used > MAX_BUFFER_SIZE {
                error!(
                    message = "Event XML exceeds maximum buffer size, cannot process",
                    buffer_size_requested = buffer_used,
                    max_buffer_size = MAX_BUFFER_SIZE
                );
                return Err(WindowsEventLogError::ReadEventError { source: e });
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
        std::slice::from_raw_parts(buffer.as_ptr() as *const u16, buffer_used as usize / 2)
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

    // Extract provider name and get properly formatted message via EvtFormatMessage
    let provider_name = EventLogSubscription::extract_provider_name(&xml);
    let rendered_message = provider_name
        .as_ref()
        .and_then(|name| format_event_message(event_handle, name));

    // Parse the XML to extract event data
    if let Ok(Some(event)) =
        EventLogSubscription::parse_event_xml(xml, &channel, &ctx.config, rendered_message)
    {
        let event_channel = event.channel.clone();

        if let Err(e) = ctx.event_sender.send(event) {
            // Check if the receiver has been dropped (channel closed)
            warn!(
                message = "Failed to send event - receiver may be closed or dropped",
                error = ?e,
                channel = %channel
            );
            // Note: We continue processing other events rather than terminating the callback
        } else {
            // Event sent successfully - update bookmark for this channel
            update_bookmark_and_checkpoint(ctx, event_channel, event_handle);
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
    fn test_extract_provider_name() {
        // Test with Provider element before EventData
        let xml = r#"
        <Event xmlns="http://schemas.microsoft.com/win/2004/08/events/event">
            <System>
                <Provider Name="Microsoft-Windows-Security-Auditing" Guid="{54849625-5478-4994-A5BA-3E3B0328C30D}"/>
                <EventID>4688</EventID>
            </System>
            <EventData>
                <Data Name="SubjectUserSid">S-1-5-18</Data>
                <Data Name="ProcessName">cmd.exe</Data>
            </EventData>
        </Event>
        "#;

        assert_eq!(
            EventLogSubscription::extract_provider_name(xml),
            Some("Microsoft-Windows-Security-Auditing".to_string())
        );

        // Test that it doesn't match Data elements
        let xml_data_first = r#"
        <Event>
            <EventData>
                <Data Name="SomeField">Value</Data>
            </EventData>
            <System>
                <Provider Name="TestProvider"/>
            </System>
        </Event>
        "#;

        assert_eq!(
            EventLogSubscription::extract_provider_name(xml_data_first),
            Some("TestProvider".to_string())
        );
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

    #[cfg(test)]
    async fn create_test_checkpointer() -> (Arc<Checkpointer>, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let checkpointer = Arc::new(Checkpointer::new(temp_dir.path()).await.unwrap());
        (checkpointer, temp_dir)
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn test_not_supported_error() {
        let config = WindowsEventLogConfig::default();
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;
        let result = EventLogSubscription::new(&config, checkpointer).await;

        assert!(matches!(
            result,
            Err(WindowsEventLogError::NotSupportedError)
        ));
    }

    #[test]
    fn test_rate_limiter_configuration() {
        // Test with rate limiting disabled (default)
        let mut config = WindowsEventLogConfig::default();
        assert_eq!(config.events_per_second, 0);

        // Test with rate limiting enabled
        config.events_per_second = 1000;
        assert_eq!(config.events_per_second, 1000);
    }

    /// Test rate limiting functionality (unit test, not integration test)
    #[cfg(windows)]
    #[tokio::test]
    async fn test_rate_limiting_delays_events() {
        use std::time::Instant;

        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.events_per_second = 10; // 10 events per second
        config.event_timeout_ms = 100; // Short timeout for test
        config.read_existing_events = true; // Read existing events for testing

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let mut subscription = EventLogSubscription::new(&config, checkpointer)
            .await
            .expect("Subscription creation should succeed");

        // Verify rate limiter was created
        assert!(subscription.rate_limiter.is_some());

        // Try to collect events and measure time
        let start = Instant::now();
        let events = subscription.next_events(20).await.unwrap_or_default();
        let elapsed = start.elapsed();

        if events.len() >= 10 {
            // If we got 10+ events, it should have taken at least ~0.9 seconds (10 events at 10/sec)
            // Using 0.8 seconds as threshold to account for timing variance
            assert!(
                elapsed.as_millis() >= 800,
                "Rate limiting should delay processing: got {} events in {:?}",
                events.len(),
                elapsed
            );
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_disabled_by_default() {
        let config = WindowsEventLogConfig::default();
        assert_eq!(
            config.events_per_second, 0,
            "Rate limiting should be disabled by default"
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
        // Security limits should prevent processing excessively large content
        let result = EventLogSubscription::extract_xml_value(&large_xml, "EventID");
        assert!(
            result.is_none(),
            "Security limits should reject excessively large XML content"
        );
    }

    #[test]
    fn test_configurable_truncation_disabled_by_default() {
        let config = WindowsEventLogConfig::default();

        // Default should be no truncation
        assert_eq!(
            config.max_event_data_length, 0,
            "Event data truncation should be disabled by default"
        );
        assert_eq!(
            config.max_message_field_length, 0,
            "Message field truncation should be disabled by default"
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

        // Test with truncation enabled
        let mut config = WindowsEventLogConfig::default();
        config.max_event_data_length = 20;

        let result = EventLogSubscription::extract_event_data(xml, &config);

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

        // Test with truncation disabled (default)
        let config = WindowsEventLogConfig::default();
        assert_eq!(
            config.max_event_data_length, 0,
            "Default should be no truncation"
        );

        let result = EventLogSubscription::extract_event_data(xml, &config);

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
    fn test_message_field_truncation_when_enabled() {
        let xml = r#"
        <Event>
            <System>
                <Provider Name="TestProvider"/>
                <EventID>1000</EventID>
                <Computer>TestComputer</Computer>
            </System>
            <EventData>
                <Data Name="Field1">This is a very long field value that will be truncated in the message summary</Data>
            </EventData>
        </Event>
        "#;

        let mut config = WindowsEventLogConfig::default();
        config.max_message_field_length = 20;

        let message = EventLogSubscription::extract_message_from_xml(
            xml,
            1000,
            "TestProvider",
            "TestComputer",
            &config,
        )
        .unwrap();

        // Message should contain truncated values
        assert!(
            message.contains("..."),
            "Message should contain truncation indicator"
        );
    }

    #[test]
    fn test_message_field_no_truncation_when_disabled() {
        let xml = r#"
        <Event>
            <System>
                <Provider Name="VeryLongProviderNameThatExceedsSixtyFourCharactersAndShouldNotBeTruncated"/>
                <EventID>1000</EventID>
                <Computer>VeryLongComputerNameThatShouldAlsoNotBeTruncatedWhenLimitIsZero</Computer>
            </System>
            <EventData>
                <Data Name="Field1">VeryLongValueThatShouldNotBeTruncated</Data>
            </EventData>
        </Event>
        "#;

        let config = WindowsEventLogConfig::default();
        assert_eq!(
            config.max_message_field_length, 0,
            "Default should be no truncation"
        );

        let message = EventLogSubscription::extract_message_from_xml(
            xml,
            1000,
            "VeryLongProviderNameThatExceedsSixtyFourCharactersAndShouldNotBeTruncated",
            "VeryLongComputerNameThatShouldAlsoNotBeTruncatedWhenLimitIsZero",
            &config,
        )
        .unwrap();

        // Message should contain full values
        assert!(
            message.contains(
                "VeryLongProviderNameThatExceedsSixtyFourCharactersAndShouldNotBeTruncated"
            )
        );
        assert!(message.contains("VeryLongValueThatShouldNotBeTruncated"));
        assert!(
            !message.contains("..."),
            "Message should not contain truncation when limit is 0"
        );
    }

    /// Integration test for invalid XPath query error handling
    /// This test verifies that invalid XPath queries are properly detected and reported
    #[cfg(windows)]
    #[tokio::test]
    async fn test_invalid_xpath_query_error() {
        use tokio::time::Duration;

        // Create a config with an intentionally invalid XPath query
        // This query has malformed syntax that Windows will reject
        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.event_query = Some("*[System[(EventID=INVALID_SYNTAX!!!".to_string()); // Deliberately malformed
        config.event_timeout_ms = 1000; // Shorter timeout for testing

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        // Create subscription - should succeed initially as EvtSubscribe doesn't validate query syntax
        let mut subscription = EventLogSubscription::new(&config, checkpointer)
            .await
            .expect("Subscription creation should succeed");

        // Give Windows time to call the callback with EvtSubscribeActionError
        // Using longer timeout for reliability on slower systems and CI environments
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Try to get events - should return an error propagated from the callback
        let result = subscription.next_events(10).await;

        match result {
            Err(WindowsEventLogError::SubscriptionError { .. }) => {
                // Success! The error was properly detected and propagated
                println!("Invalid XPath query error correctly detected and propagated");
            }
            Ok(_) => {
                panic!("Expected SubscriptionError but got success - error handling failed");
            }
            Err(other) => {
                panic!(
                    "Expected SubscriptionError but got different error: {:?}",
                    other
                );
            }
        }
    }

    /// Integration test for valid wildcard query
    /// Verifies that the fix doesn't break valid queries
    #[cfg(windows)]
    #[tokio::test]
    async fn test_valid_wildcard_query() {
        use tokio::time::Duration;

        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.event_query = Some("*".to_string()); // Valid wildcard
        config.event_timeout_ms = 2000;

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let mut subscription = EventLogSubscription::new(&config, checkpointer)
            .await
            .expect("Subscription creation should succeed");

        // Give subscription time to initialize
        // Using longer timeout for reliability on slower systems and CI environments
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Try to get events - should succeed (even if no events are available)
        let result = subscription.next_events(10).await;

        assert!(
            result.is_ok(),
            "Valid wildcard query should not produce subscription errors: {:?}",
            result
        );
    }

    /// Test for moderately complex but valid XPath queries
    /// Tests real-world filtering scenarios
    #[cfg(windows)]
    #[tokio::test]
    async fn test_valid_filtered_xpath_query() {
        use tokio::time::Duration;

        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        // Valid XPath query filtering by event level
        config.event_query = Some("*[System[Level=1 or Level=2 or Level=3]]".to_string());
        config.event_timeout_ms = 2000;

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let mut subscription = EventLogSubscription::new(&config, checkpointer)
            .await
            .expect("Subscription creation should succeed");

        // Give subscription time to initialize
        // Using longer timeout for reliability on slower systems and CI environments
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Try to get events - should succeed
        let result = subscription.next_events(10).await;

        assert!(
            result.is_ok(),
            "Valid filtered XPath query should not produce subscription errors: {:?}",
            result
        );
    }

    /// Regression test for multiple concurrent subscriptions
    /// Verifies that multiple EventLogSubscription instances don't interfere with each other
    /// This tests the fix for the global context bug where subscriptions would overwrite each other
    #[cfg(windows)]
    #[tokio::test]
    async fn test_multiple_concurrent_subscriptions() {
        use tokio::time::Duration;

        // Create three separate subscriptions (simulating real-world multi-source config)
        let mut config1 = WindowsEventLogConfig::default();
        config1.channels = vec!["Application".to_string()];
        config1.event_timeout_ms = 2000;

        let mut config2 = WindowsEventLogConfig::default();
        config2.channels = vec!["System".to_string()];
        config2.event_timeout_ms = 2000;

        let mut config3 = WindowsEventLogConfig::default();
        config3.channels = vec!["Security".to_string()];
        config3.event_timeout_ms = 2000;

        let (checkpointer1, _temp_dir1) = create_test_checkpointer().await;
        let (checkpointer2, _temp_dir2) = create_test_checkpointer().await;
        let (checkpointer3, _temp_dir3) = create_test_checkpointer().await;

        // Create all three subscriptions concurrently
        let mut sub1 = EventLogSubscription::new(&config1, checkpointer1)
            .await
            .expect("Subscription 1 (Application) should succeed");
        let mut sub2 = EventLogSubscription::new(&config2, checkpointer2)
            .await
            .expect("Subscription 2 (System) should succeed");
        let mut sub3 = EventLogSubscription::new(&config3, checkpointer3)
            .await
            .expect("Subscription 3 (Security) should succeed");

        // Give all subscriptions time to initialize
        // Using longer timeout for reliability on slower systems and CI environments
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Try to get events from each subscription independently
        // All should succeed without interfering with each other
        let result1 = sub1.next_events(5).await;
        let result2 = sub2.next_events(5).await;
        let result3 = sub3.next_events(5).await;

        assert!(
            result1.is_ok(),
            "Subscription 1 (Application) should not error: {:?}",
            result1
        );
        assert!(
            result2.is_ok(),
            "Subscription 2 (System) should not error: {:?}",
            result2
        );
        assert!(
            result3.is_ok(),
            "Subscription 3 (Security) should not error: {:?}",
            result3
        );

        // Verify subscriptions are independent by collecting events again
        // If contexts were shared/overwritten, callbacks would route to wrong channels
        let result1_again = sub1.next_events(5).await;
        let result2_again = sub2.next_events(5).await;
        let result3_again = sub3.next_events(5).await;

        assert!(
            result1_again.is_ok(),
            "Subscription 1 should remain healthy"
        );
        assert!(
            result2_again.is_ok(),
            "Subscription 2 should remain healthy"
        );
        assert!(
            result3_again.is_ok(),
            "Subscription 3 should remain healthy"
        );
    }

    /// Regression test for read_existing_events=false behavior
    ///
    /// This test verifies that when read_existing_events=false and there's no checkpoint,
    /// the subscription only receives NEW events (EvtSubscribeToFutureEvents), not
    /// historical events from the beginning of the log.
    ///
    /// Bug fixed: Previously, we always created a BookmarkManager object, so has_bookmark
    /// was always true. With EvtSubscribeStartAfterBookmark and an empty bookmark, Windows
    /// starts from the oldest record, ignoring read_existing_events=false.
    #[cfg(windows)]
    #[tokio::test]
    async fn test_read_existing_events_false_only_receives_future_events() {
        use chrono::Utc;
        use tokio::time::Duration;

        // Create config with read_existing_events=false (the default, but be explicit)
        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.read_existing_events = false; // Explicitly false - only future events
        config.event_timeout_ms = 500; // Short timeout since we expect few/no events

        // Use a fresh temp directory with NO checkpoint file
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        // Record the time BEFORE creating the subscription
        let subscription_start_time = Utc::now();

        let mut subscription = EventLogSubscription::new(&config, checkpointer)
            .await
            .expect("Subscription creation should succeed");

        // Give a brief moment for subscription to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Try to get events - with read_existing_events=false, we should only get
        // events that occurred AFTER the subscription was created
        let result = subscription.next_events(100).await;
        assert!(result.is_ok(), "Should not error: {:?}", result);

        let events = result.unwrap();

        // If we got any events, they should all have timestamps AFTER we created
        // the subscription (within a reasonable tolerance for clock skew)
        let tolerance = chrono::Duration::seconds(5);
        let earliest_allowed = subscription_start_time - tolerance;

        for event in &events {
            assert!(
                event.time_created >= earliest_allowed,
                "Event timestamp {} is before subscription start time {} (minus {}s tolerance). \
                 This suggests read_existing_events=false is not being respected and historical \
                 events are being read. Event ID: {}, Record ID: {}",
                event.time_created,
                subscription_start_time,
                tolerance.num_seconds(),
                event.event_id,
                event.record_id
            );
        }

        // Note: We can't assert events.is_empty() because new events might genuinely
        // occur during the test. The key assertion is that any events we DO receive
        // have recent timestamps, not timestamps from days/weeks ago.
    }

    /// Regression test for read_existing_events=true behavior
    ///
    /// This test verifies that when read_existing_events=true and there's no checkpoint,
    /// the subscription DOES receive historical events from the log.
    #[cfg(windows)]
    #[tokio::test]
    async fn test_read_existing_events_true_receives_historical_events() {
        use chrono::Utc;
        use tokio::time::Duration;

        // Create config with read_existing_events=true
        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.read_existing_events = true; // Read from beginning of log
        config.event_timeout_ms = 2000;

        // Use a fresh temp directory with NO checkpoint file
        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let subscription_start_time = Utc::now();

        let mut subscription = EventLogSubscription::new(&config, checkpointer)
            .await
            .expect("Subscription creation should succeed");

        // Give time to receive historical events
        tokio::time::sleep(Duration::from_millis(500)).await;

        let result = subscription.next_events(100).await;
        assert!(result.is_ok(), "Should not error: {:?}", result);

        let events = result.unwrap();

        // With read_existing_events=true, we should receive events
        // The Application log typically has many historical events
        assert!(
            !events.is_empty(),
            "With read_existing_events=true, we should receive historical events from the log"
        );

        // At least some events should have timestamps BEFORE the subscription started
        // (proving we're reading historical events, not just future ones)
        let has_historical = events
            .iter()
            .any(|e| e.time_created < subscription_start_time);

        assert!(
            has_historical,
            "With read_existing_events=true, at least some events should be historical \
             (timestamps before {}). Got {} events, oldest timestamp: {:?}",
            subscription_start_time,
            events.len(),
            events.iter().map(|e| e.time_created).min()
        );
    }

    /// Test that checkpoint restoration correctly uses bookmark-based resume
    #[cfg(windows)]
    #[tokio::test]
    async fn test_checkpoint_restoration_uses_bookmark() {
        use tokio::time::Duration;

        // Create a checkpointer and manually set a checkpoint
        let temp_dir = tempfile::TempDir::new().unwrap();
        let checkpointer = Arc::new(Checkpointer::new(temp_dir.path()).await.unwrap());

        // Create a fake bookmark XML (this is what Windows bookmark XML looks like)
        // Using a very high record ID to ensure we don't receive any events
        let fake_bookmark = r#"<BookmarkList><Bookmark Channel='Application' RecordId='999999999' IsCurrent='true'/></BookmarkList>"#;

        checkpointer
            .set("Application".to_string(), fake_bookmark.to_string())
            .await
            .expect("Should be able to set checkpoint");

        // Now create a subscription - it should use the checkpoint
        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.read_existing_events = true; // Even with this true, checkpoint should take precedence
        config.event_timeout_ms = 500;

        let mut subscription = EventLogSubscription::new(&config, checkpointer)
            .await
            .expect("Subscription creation should succeed");

        tokio::time::sleep(Duration::from_millis(200)).await;

        let result = subscription.next_events(100).await;
        assert!(result.is_ok(), "Should not error: {:?}", result);

        let events = result.unwrap();

        // With a checkpoint pointing to record 999999999, we should receive very few
        // or no events (since that record ID likely doesn't exist yet)
        // This proves the checkpoint is being used, not read_existing_events
        assert!(
            events.len() < 10,
            "With checkpoint at record 999999999, we should receive few/no events. \
             Got {} events, suggesting checkpoint was ignored and read_existing_events=true \
             is reading from the beginning.",
            events.len()
        );
    }

    /// Test bookmark XML validation logic
    ///
    /// This verifies that empty/invalid bookmark XML is not saved as a checkpoint,
    /// which prevents the bug where channels with no events get "empty" bookmarks
    /// saved, causing them to read from the beginning on restart.
    #[test]
    fn test_is_valid_bookmark_xml() {
        // Valid bookmark XML (actual Windows format)
        let valid = r#"<BookmarkList>
  <Bookmark Channel='Application' RecordId='12345' IsCurrent='true'/>
</BookmarkList>"#;
        assert!(
            EventLogSubscription::is_valid_bookmark_xml(valid),
            "Should accept valid bookmark with RecordId"
        );

        // Empty string
        assert!(
            !EventLogSubscription::is_valid_bookmark_xml(""),
            "Should reject empty string"
        );

        // Empty BookmarkList (what Windows returns for never-updated bookmark)
        let empty_list = "<BookmarkList/>";
        assert!(
            !EventLogSubscription::is_valid_bookmark_xml(empty_list),
            "Should reject empty BookmarkList"
        );

        // BookmarkList with no bookmarks
        let empty_list2 = "<BookmarkList></BookmarkList>";
        assert!(
            !EventLogSubscription::is_valid_bookmark_xml(empty_list2),
            "Should reject BookmarkList without Bookmark element"
        );

        // Bookmark without RecordId (malformed)
        let no_record_id = "<BookmarkList><Bookmark Channel='System'/></BookmarkList>";
        assert!(
            !EventLogSubscription::is_valid_bookmark_xml(no_record_id),
            "Should reject Bookmark without RecordId"
        );
    }

    /// Test that parse_event_xml uses pre-rendered message when provided
    #[test]
    fn test_parse_event_xml_uses_pre_rendered_message() {
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
        let pre_rendered = Some("Pre-rendered message from EvtFormatMessage".to_string());

        let result = EventLogSubscription::parse_event_xml(
            xml.to_string(),
            "Application",
            &config,
            pre_rendered,
        );

        let event = result.unwrap().unwrap();
        assert_eq!(
            event.rendered_message,
            Some("Pre-rendered message from EvtFormatMessage".to_string()),
            "Should use pre-rendered message when provided"
        );
    }

    /// Test that parse_event_xml falls back to XML extraction when no pre-rendered message
    #[test]
    fn test_parse_event_xml_fallback_without_pre_rendered() {
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

        let result =
            EventLogSubscription::parse_event_xml(xml.to_string(), "Application", &config, None);

        let event = result.unwrap().unwrap();
        assert!(
            event.rendered_message.is_some(),
            "Should fall back to XML-based message extraction"
        );
        assert!(
            event.rendered_message.as_ref().unwrap().contains("1000"),
            "Fallback message should contain event ID"
        );
    }

    /// Integration test: Verify EvtFormatMessage produces meaningful messages
    /// for real Windows events (Application channel typically has events)
    #[cfg(windows)]
    #[tokio::test]
    async fn test_evt_format_message_produces_real_messages() {
        use tokio::time::Duration;

        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.read_existing_events = true; // Read historical events
        config.event_timeout_ms = 2000;

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let mut subscription = EventLogSubscription::new(&config, checkpointer)
            .await
            .expect("Subscription should succeed");

        tokio::time::sleep(Duration::from_millis(500)).await;

        let result = subscription.next_events(50).await;
        assert!(result.is_ok(), "Should get events: {:?}", result);

        let events = result.unwrap();
        if events.is_empty() {
            // Skip test if no events available (unlikely but possible)
            return;
        }

        // Check that at least some events have properly formatted messages
        // (not just "Event ID X from Y on Z" fallback pattern)
        let has_formatted_message = events.iter().any(|e| {
            e.rendered_message
                .as_ref()
                .map(|msg| {
                    // Formatted messages typically don't follow our fallback pattern
                    !msg.starts_with("Event ID ")
                })
                .unwrap_or(false)
        });

        // Note: This assertion may fail if all events come from providers
        // without message tables, which is rare but possible
        assert!(
            has_formatted_message,
            "Expected at least some events to have EvtFormatMessage-rendered messages. \
             Got {} events, all with fallback messages. This may indicate EvtFormatMessage \
             is not working correctly.",
            events.len()
        );
    }

    /// Test that exact channel names pass through expand_channel_patterns unchanged
    #[test]
    fn test_expand_channel_patterns_exact_names() {
        let patterns = vec![
            "System".to_string(),
            "Application".to_string(),
            "Security".to_string(),
        ];

        let result = expand_channel_patterns(&patterns);
        assert!(result.is_ok());

        let expanded = result.unwrap();
        assert_eq!(
            expanded, patterns,
            "Exact names should pass through unchanged"
        );
    }

    /// Integration test: Verify wildcard patterns expand to real channels
    #[cfg(windows)]
    #[test]
    fn test_enumerate_all_channels_returns_channels() {
        let result = enumerate_all_channels();
        assert!(result.is_ok(), "Channel enumeration should succeed");

        let channels = result.unwrap();
        assert!(!channels.is_empty(), "Should find at least some channels");

        // Common channels that should exist on any Windows system
        let has_system = channels.iter().any(|c| c == "System");
        let has_application = channels.iter().any(|c| c == "Application");

        assert!(has_system, "System channel should exist");
        assert!(has_application, "Application channel should exist");
    }

    /// Integration test: Verify wildcard pattern expansion works
    #[cfg(windows)]
    #[test]
    fn test_expand_channel_patterns_with_wildcards() {
        // Test with a pattern that should match multiple channels
        let patterns = vec!["System".to_string(), "Microsoft-Windows-*".to_string()];

        let result = expand_channel_patterns(&patterns);
        assert!(result.is_ok(), "Pattern expansion should succeed");

        let expanded = result.unwrap();

        // Should include exact match
        assert!(
            expanded.contains(&"System".to_string()),
            "Should include exact match 'System'"
        );

        // Should have expanded the wildcard to multiple channels
        assert!(
            expanded.len() > 2,
            "Wildcard should expand to multiple channels, got {}",
            expanded.len()
        );

        // All expanded channels starting with Microsoft-Windows- should exist
        let ms_channels: Vec<_> = expanded
            .iter()
            .filter(|c| c.starts_with("Microsoft-Windows-"))
            .collect();
        assert!(
            !ms_channels.is_empty(),
            "Should have matched some Microsoft-Windows-* channels"
        );
    }

    /// Integration test: Create subscription with wildcard patterns
    #[cfg(windows)]
    #[tokio::test]
    async fn test_subscription_with_wildcard_patterns() {
        let mut config = WindowsEventLogConfig::default();
        // Use a pattern that will match System and Application
        config.channels = vec![
            "Sys*".to_string(), // Should match "System"
            "Application".to_string(),
        ];
        config.read_existing_events = false;
        config.event_timeout_ms = 1000;

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let result = EventLogSubscription::new(&config, checkpointer).await;
        assert!(
            result.is_ok(),
            "Subscription with wildcard should succeed: {:?}",
            result.err()
        );
    }
}
