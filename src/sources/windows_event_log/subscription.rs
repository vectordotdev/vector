use std::{
    collections::HashMap,
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
};

use lru::LruCache;

use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use metrics::{Counter, Gauge, counter, gauge};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::EventLog::{
    EVT_HANDLE, EvtClose, EvtNext, EvtOpenChannelConfig, EvtSubscribe,
    EvtSubscribeStartAfterBookmark, EvtSubscribeStartAtOldestRecord, EvtSubscribeStrict,
    EvtSubscribeToFutureEvents,
};
#[cfg(test)]
use windows::Win32::System::Threading::SetEvent;
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, WaitForMultipleObjects};
use windows::core::HSTRING;

use super::{
    bookmark::BookmarkManager, checkpoint::Checkpointer, config::WindowsEventLogConfig, error::*,
    metadata, sid_resolver::SidResolver, xml_parser,
};

use crate::internal_events::WindowsEventLogBookmarkError;

/// Maximum number of entries in the EvtFormatMessage result cache.
pub const FORMAT_CACHE_CAPACITY: usize = 10_000;
/// Maximum number of cached publisher metadata handles.
const PUBLISHER_CACHE_CAPACITY: usize = 256;

/// RAII wrapper for EvtOpenPublisherMetadata handles.
/// Calls EvtClose on drop to prevent handle leaks when evicted from LRU cache.
pub struct PublisherHandle(pub isize);

impl Drop for PublisherHandle {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe {
                let _ = EvtClose(EVT_HANDLE(self.0));
            }
        }
    }
}

// Win32 error codes extracted from the lower 16 bits of HRESULT.
// Using named constants instead of magic numbers for maintainability.
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_NO_MORE_ITEMS: u32 = 259;
const ERROR_EVT_QUERY_RESULT_STALE: u32 = 4317;
const ERROR_EVT_CHANNEL_NOT_FOUND: u32 = 0x3AA1; // 15009
const ERROR_EVT_INVALID_QUERY: u32 = 15007;
const ERROR_EVT_QUERY_RESULT_INVALID_POSITION: u32 = 0x4239; // 16953

/// Per-channel subscription state for pull model.
struct ChannelSubscription {
    channel: String,
    subscription_handle: EVT_HANDLE,
    signal_event: HANDLE,
    bookmark: BookmarkManager,
    /// Pre-registered counter for events read on this channel.
    events_read_counter: Counter,
    /// Pre-registered counter for render errors on this channel.
    render_errors_counter: Counter,
    /// Gauge indicating whether this channel subscription is active (1.0) or failed (0.0).
    subscription_active_gauge: Gauge,
    /// Gauge tracking the timestamp (unix seconds) of the last event received on this channel.
    last_event_timestamp_gauge: Gauge,
    /// Gauge tracking total record count in the channel log.
    /// SOC teams use `rate(events_read_total)` vs this gauge to detect ingestion lag.
    channel_records_gauge: Gauge,
}

// SAFETY: Same rationale as EventLogSubscription - Windows kernel handles are thread-safe.
unsafe impl Send for ChannelSubscription {}

/// Result of waiting for events across all channels.
pub enum WaitResult {
    /// At least one channel has events available.
    EventsAvailable,
    /// Timeout expired without any events.
    Timeout,
    /// Shutdown was signaled.
    Shutdown,
}

/// Pull-model Windows Event Log subscription using EvtSubscribe + signal event + EvtNext.
///
/// Instead of a callback (push model), we use:
/// 1. `CreateEventW` to create a manual-reset signal per channel
/// 2. `EvtSubscribe` with NULL callback (pull mode) and signal event
/// 3. `WaitForMultipleObjects` to wait for any channel signal or shutdown
/// 4. `EvtNext` to pull events in batches when signaled
///
/// This eliminates event drops under back pressure because we don't call
/// `EvtNext` again until the pipeline has consumed the current batch.
pub struct EventLogSubscription {
    config: Arc<WindowsEventLogConfig>,
    channels: Vec<ChannelSubscription>,
    checkpointer: Arc<Checkpointer>,
    rate_limiter: Option<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
    shutdown_event: HANDLE,
    render_buffer: Vec<u8>,
    /// Cached EvtOpenPublisherMetadata handles keyed by provider name.
    /// Bounded LRU; evicted handles are closed via `PublisherHandle::drop`.
    publisher_cache: LruCache<String, PublisherHandle>,
    /// Cached EvtFormatMessage results. Outer key is provider name (looked up
    /// via `&str` — zero allocation on the hot path), inner LRU is bounded per provider.
    format_cache: HashMap<String, LruCache<(u32, u64), Option<String>>>,
    /// Pre-registered counter for metadata cache hits.
    cache_hits_counter: Counter,
    /// Pre-registered counter for metadata cache misses.
    cache_misses_counter: Counter,
    /// SID-to-username resolver with LRU cache.
    sid_resolver: SidResolver,
    /// Reusable UTF-16 decode buffer to avoid per-event allocations.
    decode_buffer: Vec<u16>,
    /// Round-robin index for fair channel scheduling. Rotates the starting
    /// channel each pull_events call to prevent a single busy channel
    /// (e.g., Security on a domain controller) from starving others.
    round_robin_index: usize,
}

// SAFETY: Windows HANDLE and EVT_HANDLE are kernel objects safe to use across
// threads. In windows 0.58, HANDLE wraps *mut c_void which is !Send/!Sync,
// but the underlying kernel handles are thread-safe.
unsafe impl Send for EventLogSubscription {}

impl EventLogSubscription {
    /// Create a new pull-model subscription for all configured channels.
    ///
    /// Each channel gets its own signal event and EvtSubscribe handle.
    /// A shutdown event is created for clean termination of blocking waits.
    pub async fn new(
        config: &WindowsEventLogConfig,
        checkpointer: Arc<Checkpointer>,
        _acknowledgements: bool,
    ) -> Result<Self, WindowsEventLogError> {
        // Create rate limiter if configured
        let rate_limiter = if config.events_per_second > 0 {
            NonZeroU32::new(config.events_per_second).map(|rate| {
                info!(
                    message = "Enabling rate limiting for Windows Event Log source.",
                    events_per_second = config.events_per_second
                );
                RateLimiter::direct(Quota::per_second(rate))
            })
        } else {
            None
        };

        let config = Arc::new(config.clone());

        // Validate channels exist and are accessible
        Self::validate_channels(&config)?;

        // Store as isize while held across await points (HANDLE wraps *mut c_void which is !Send)
        let shutdown_event_raw: isize = unsafe {
            let h = CreateEventW(None, true, false, None).map_err(|e| {
                WindowsEventLogError::ConfigError {
                    message: format!("Failed to create shutdown event: {e}"),
                }
            })?;
            h.0 as isize
        };

        let mut channel_subscriptions = Vec::with_capacity(config.channels.len());

        for channel in &config.channels {
            // Initialize bookmark from checkpoint or create fresh
            let (bookmark, has_valid_checkpoint) = if let Some(checkpoint) =
                checkpointer.get(channel).await
            {
                match BookmarkManager::from_xml(&checkpoint.bookmark_xml) {
                    Ok(bm) => {
                        info!(
                            message = "Resuming from checkpoint bookmark.",
                            channel = %channel
                        );
                        (bm, true)
                    }
                    Err(e) => {
                        warn!(
                            message = "Corrupted bookmark XML in checkpoint, creating fresh bookmark. Potential re-delivery of events.",
                            channel = %channel,
                            error = %e
                        );
                        (BookmarkManager::new()?, false)
                    }
                }
            } else {
                info!(
                    message = "No checkpoint found, creating fresh bookmark.",
                    channel = %channel
                );
                (BookmarkManager::new()?, false)
            };

            // Create manual-reset signal event, initially signaled.
            // Initially signaled ensures the first iteration drains any buffered events.
            // Manual reset prevents missing signals between WaitForMultipleObjects return
            // and EvtNext draining.
            let signal_event = unsafe {
                CreateEventW(None, true, true, None).map_err(|e| {
                    WindowsEventLogError::ConfigError {
                        message: format!(
                            "Failed to create signal event for channel '{channel}': {e}"
                        ),
                    }
                })?
            };

            let channel_hstring = HSTRING::from(channel.as_str());
            let query = Self::build_xpath_query(&config)?;
            let query_hstring = HSTRING::from(query.clone());

            // Determine subscription flags.
            // When resuming from a bookmark, OR in EvtSubscribeStrict (0x10000) so that
            // Windows fails explicitly if the bookmark position is stale/invalid,
            // rather than silently falling back to oldest-record.
            let subscription_flags = if has_valid_checkpoint {
                EvtSubscribeStartAfterBookmark.0 | EvtSubscribeStrict.0
            } else if config.read_existing_events {
                EvtSubscribeStartAtOldestRecord.0
            } else {
                EvtSubscribeToFutureEvents.0
            };

            let fallback_flags = if config.read_existing_events {
                EvtSubscribeStartAtOldestRecord.0
            } else {
                EvtSubscribeToFutureEvents.0
            };

            debug!(
                message = "Creating pull-mode subscription.",
                channel = %channel,
                query = %query,
                has_valid_checkpoint = has_valid_checkpoint,
                read_existing = config.read_existing_events,
                flags = format!("{:#x}", subscription_flags)
            );

            // EvtSubscribe with signal event and NULL callback = pull mode
            let bookmark_handle = bookmark.as_handle();
            let subscription_result = unsafe {
                if has_valid_checkpoint {
                    let strict_result = EvtSubscribe(
                        None,
                        signal_event,
                        &channel_hstring,
                        &query_hstring,
                        bookmark_handle,
                        None, // NULL context = pull mode
                        None, // NULL callback = pull mode
                        subscription_flags,
                    );
                    match strict_result {
                        Ok(handle) => Ok(handle),
                        Err(e) => {
                            warn!(
                                message = "Strict bookmark subscribe failed, retrying without bookmark. Potential re-delivery of events.",
                                channel = %channel,
                                error = %e,
                                fallback_flags = format!("{:#x}", fallback_flags)
                            );
                            EvtSubscribe(
                                None,
                                signal_event,
                                &channel_hstring,
                                &query_hstring,
                                None, // No bookmark for fallback
                                None,
                                None,
                                fallback_flags,
                            )
                        }
                    }
                } else {
                    EvtSubscribe(
                        None,
                        signal_event,
                        &channel_hstring,
                        &query_hstring,
                        None, // No bookmark for fresh start
                        None, // NULL context
                        None, // NULL callback
                        subscription_flags,
                    )
                }
            };

            match subscription_result {
                Ok(subscription_handle) => {
                    info!(
                        message = "Pull-mode subscription created successfully.",
                        channel = %channel
                    );
                    counter!(
                        "windows_event_log_subscriptions_total",
                        "channel" => channel.clone()
                    )
                    .increment(1);
                    let subscription_active_gauge = gauge!(
                        "windows_event_log_subscription_active",
                        "channel" => channel.clone()
                    );
                    subscription_active_gauge.set(1.0);

                    channel_subscriptions.push(ChannelSubscription {
                        channel: channel.clone(),
                        events_read_counter: counter!(
                            "windows_event_log_events_read_total",
                            "channel" => channel.clone()
                        ),
                        render_errors_counter: counter!(
                            "windows_event_log_render_errors_total",
                            "channel" => channel.clone()
                        ),
                        subscription_active_gauge,
                        last_event_timestamp_gauge: gauge!(
                            "windows_event_log_last_event_timestamp_seconds",
                            "channel" => channel.clone()
                        ),
                        channel_records_gauge: gauge!(
                            "windows_event_log_channel_records_total",
                            "channel" => channel.clone()
                        ),
                        subscription_handle,
                        signal_event,
                        bookmark,
                    });
                }
                Err(e) => {
                    let error_code = (e.code().0 as u32) & 0xFFFF;
                    if error_code == ERROR_EVT_CHANNEL_NOT_FOUND
                        || error_code == ERROR_EVT_INVALID_QUERY
                    {
                        warn!(
                            message = "Skipping channel (not found or invalid query).",
                            channel = %channel,
                            error_code = error_code
                        );
                        unsafe {
                            let _ = CloseHandle(signal_event);
                        }
                        continue;
                    } else if error_code == ERROR_ACCESS_DENIED {
                        warn!(
                            message = "Skipping channel due to access denied.",
                            channel = %channel
                        );
                        unsafe {
                            let _ = CloseHandle(signal_event);
                        }
                        continue;
                    } else {
                        // Clean up already-created subscriptions on failure
                        for sub in channel_subscriptions {
                            unsafe {
                                let _ = EvtClose(sub.subscription_handle);
                                let _ = CloseHandle(sub.signal_event);
                            }
                        }
                        unsafe {
                            let _ =
                                CloseHandle(HANDLE(shutdown_event_raw as *mut std::ffi::c_void));
                        }
                        return Err(WindowsEventLogError::CreateSubscriptionError { source: e });
                    }
                }
            }
        }

        // Verify we subscribed to at least one channel
        if channel_subscriptions.is_empty() {
            unsafe {
                let _ = CloseHandle(HANDLE(shutdown_event_raw as *mut std::ffi::c_void));
            }
            return Err(WindowsEventLogError::ConfigError {
                message: "No channels could be subscribed to. All channels may be inaccessible or direct/analytic channels.".into(),
            });
        }

        info!(
            message = "Successfully subscribed to channels (pull mode).",
            channel_count = channel_subscriptions.len()
        );

        let shutdown_event = HANDLE(shutdown_event_raw as *mut std::ffi::c_void);
        Ok(Self {
            config,
            channels: channel_subscriptions,
            checkpointer,
            rate_limiter,
            shutdown_event,
            render_buffer: vec![0u8; 16384],
            publisher_cache: LruCache::new(NonZeroUsize::new(PUBLISHER_CACHE_CAPACITY).unwrap()),
            format_cache: HashMap::new(),
            cache_hits_counter: counter!("windows_event_log_cache_hits_total"),
            cache_misses_counter: counter!("windows_event_log_cache_misses_total"),
            sid_resolver: SidResolver::new(),
            decode_buffer: vec![0u16; 8192],
            round_robin_index: 0,
        })
    }

    /// Wait for events to become available on any channel, or for shutdown.
    ///
    /// Uses `WaitForMultipleObjects` via `spawn_blocking` to avoid blocking the
    /// Tokio runtime. The wait array includes all channel signal events plus the
    /// shutdown event.
    pub fn wait_for_events_blocking(&self, timeout_ms: u32) -> WaitResult {
        // Build wait handle array: [channel0_signal, channel1_signal, ..., shutdown_event]
        let mut handles: Vec<HANDLE> = self.channels.iter().map(|c| c.signal_event).collect();
        handles.push(self.shutdown_event);

        let result = unsafe { WaitForMultipleObjects(&handles, false, timeout_ms) };

        let shutdown_index = (self.channels.len()) as u32;

        match result {
            r if r == WAIT_TIMEOUT => WaitResult::Timeout,
            r if r.0 < WAIT_OBJECT_0.0 + shutdown_index => WaitResult::EventsAvailable,
            r if r.0 == WAIT_OBJECT_0.0 + shutdown_index => WaitResult::Shutdown,
            _ => {
                // WAIT_FAILED or unexpected - treat as timeout to avoid tight loop
                warn!(
                    message = "WaitForMultipleObjects returned unexpected result.",
                    result = result.0
                );
                WaitResult::Timeout
            }
        }
    }

    /// Pull events from all signaled channels with fair scheduling.
    ///
    /// Each channel gets a per-channel budget of `max_events / num_channels`
    /// to prevent a single busy channel (e.g., Security) from starving others.
    /// The starting channel rotates each call via round-robin. Channels that
    /// don't use their budget simply leave slots unused — the next pull_events
    /// call reclaims them naturally since the signal stays set.
    ///
    /// # At-least-once delivery semantics
    ///
    /// If a bookmark update fails mid-batch, events processed *before* the
    /// failure are still returned and sent downstream, but the bookmark position
    /// does not advance. On restart, those events will be re-read from the
    /// channel, resulting in duplicates. This is an intentional trade-off:
    /// at-least-once delivery is preferable to data loss.
    pub fn pull_events(
        &mut self,
        max_events: usize,
    ) -> Result<Vec<xml_parser::WindowsEvent>, WindowsEventLogError> {
        let mut all_events = Vec::with_capacity(max_events.min(1000));
        let num_channels = self.channels.len().max(1);
        let per_channel_budget = (max_events / num_channels).max(1);
        let start = self.round_robin_index % num_channels;
        self.round_robin_index = self.round_robin_index.wrapping_add(1);

        for i in 0..num_channels {
            let channel_idx = (start + i) % num_channels;
            let channel_sub = &mut self.channels[channel_idx];
            let channel_limit = per_channel_budget.min(max_events.saturating_sub(all_events.len()));

            if channel_limit == 0 {
                break;
            }

            let mut channel_drained = false;
            let mut bookmark_failed = false;
            let mut channel_count = 0usize;

            // Drain loop: keep calling EvtNext until ERROR_NO_MORE_ITEMS or channel budget.
            // Only reset the signal once the channel is fully drained; if we hit the
            // budget limit the signal stays set so WaitForMultipleObjects returns immediately.
            'drain: loop {
                if channel_count >= channel_limit {
                    break;
                }

                let batch_size = (channel_limit - channel_count).min(100);
                let mut event_handles: Vec<isize> = vec![0isize; batch_size];
                let mut returned: u32 = 0;

                let result = unsafe {
                    EvtNext(
                        channel_sub.subscription_handle,
                        &mut event_handles,
                        0,
                        0,
                        &mut returned,
                    )
                };

                if let Err(err) = result {
                    let code = (err.code().0 as u32) & 0xFFFF;
                    if code == ERROR_NO_MORE_ITEMS {
                        channel_drained = true;
                        break;
                    }
                    if code == ERROR_EVT_QUERY_RESULT_STALE {
                        debug!(
                            message = "Channel subscription ended.",
                            channel = %channel_sub.channel
                        );
                        channel_drained = true;
                        break;
                    }
                    if code == ERROR_EVT_QUERY_RESULT_INVALID_POSITION {
                        warn!(
                            message = "Event log channel was cleared or query position invalidated, attempting re-subscription.",
                            channel = %channel_sub.channel
                        );
                        match Self::resubscribe_channel(channel_sub, &self.config) {
                            Ok(()) => {
                                info!(
                                    message = "Re-subscription succeeded after stale query.",
                                    channel = %channel_sub.channel
                                );
                                // Retry from fresh subscription — the signal will fire again
                                channel_drained = true;
                                break;
                            }
                            Err(e) => {
                                warn!(
                                    message = "Re-subscription failed, will retry next cycle.",
                                    channel = %channel_sub.channel,
                                    error = %e
                                );
                                channel_sub.subscription_active_gauge.set(0.0);
                                channel_drained = true;
                                break;
                            }
                        }
                    }
                    return Err(WindowsEventLogError::PullEventsError {
                        channel: channel_sub.channel.clone(),
                        source: err,
                    });
                }

                if returned == 0 {
                    channel_drained = true;
                    break;
                }

                channel_sub.events_read_counter.increment(returned as u64);
                channel_sub
                    .last_event_timestamp_gauge
                    .set(chrono::Utc::now().timestamp() as f64);

                let batch_handles = &event_handles[..returned as usize];
                for (idx, &raw_handle) in batch_handles.iter().enumerate() {
                    let event_handle = EVT_HANDLE(raw_handle);

                    match super::render::render_event_xml(
                        &mut self.render_buffer,
                        &mut self.decode_buffer,
                        event_handle,
                    ) {
                        Ok(xml) => {
                            // Single-pass: parse all System fields in one traversal
                            let system_fields = xml_parser::parse_system_section(&xml);

                            // Early pre-filter: discard non-matching event IDs before
                            // the expensive resolve_event_metadata / format_event_message
                            // calls. This guarantees improved performance even when
                            // XPath-level filtering is not applied (e.g. large ID lists).
                            if let Some(ref only_ids) = self.config.only_event_ids
                                && !only_ids.contains(&system_fields.event_id)
                            {
                                counter!("windows_event_log_events_filtered_total", "reason" => "event_id_prefilter")
                                    .increment(1);
                                unsafe {
                                    let _ = EvtClose(event_handle);
                                }
                                continue;
                            }
                            if self
                                .config
                                .ignore_event_ids
                                .contains(&system_fields.event_id)
                            {
                                counter!("windows_event_log_events_filtered_total", "reason" => "event_id_prefilter")
                                    .increment(1);
                                unsafe {
                                    let _ = EvtClose(event_handle);
                                }
                                continue;
                            }

                            let channel_name = if system_fields.channel.is_empty() {
                                channel_sub.channel.clone()
                            } else {
                                system_fields.channel.clone()
                            };
                            let provider_name = system_fields.provider_name.clone();
                            let task_val = system_fields.task as u64;
                            let opcode_val = system_fields.opcode as u64;
                            let keywords_val = system_fields.keywords;

                            let (task_name, opcode_name, keyword_names) =
                                if !provider_name.is_empty() {
                                    metadata::resolve_event_metadata(
                                        &mut self.publisher_cache,
                                        &mut self.format_cache,
                                        &self.cache_hits_counter,
                                        &self.cache_misses_counter,
                                        event_handle,
                                        &provider_name,
                                        task_val,
                                        opcode_val,
                                        keywords_val,
                                    )
                                } else {
                                    (None, None, Vec::new())
                                };

                            let rendered_message =
                                if self.config.render_message && !provider_name.is_empty() {
                                    metadata::format_event_message(
                                        &mut self.publisher_cache,
                                        event_handle,
                                        &provider_name,
                                    )
                                } else {
                                    None
                                };

                            if let Ok(Some(mut event)) = xml_parser::build_event(
                                xml,
                                &channel_name,
                                &self.config,
                                rendered_message,
                                system_fields,
                            ) {
                                event.task_name = task_name;
                                event.opcode_name = opcode_name;
                                event.keyword_names = keyword_names;

                                // Resolve SID to human-readable account name
                                if let Some(ref sid) = event.user_id {
                                    if let Some(account_name) = self.sid_resolver.resolve(sid) {
                                        event.user_name = Some(account_name);
                                    }
                                }

                                if let Err(e) = channel_sub.bookmark.update(event_handle) {
                                    emit!(WindowsEventLogBookmarkError {
                                        channel: channel_sub.channel.clone(),
                                        error: e.to_string(),
                                    });
                                    bookmark_failed = true;
                                    // Events already in all_events will still be delivered
                                    // (at-least-once semantics — see doc comment on pull_events).
                                    // Close current handle normally
                                    unsafe {
                                        let _ = EvtClose(event_handle);
                                    }
                                    // Close remaining unprocessed handles to prevent leak
                                    for &h in &batch_handles[idx + 1..] {
                                        unsafe {
                                            let _ = EvtClose(EVT_HANDLE(h));
                                        }
                                    }
                                    break 'drain;
                                }
                                all_events.push(event);
                                channel_count += 1;
                            }
                        }
                        Err(e) => {
                            channel_sub.render_errors_counter.increment(1);
                            warn!(
                                message = "Failed to render event XML.",
                                channel = %channel_sub.channel,
                                batch_index = idx,
                                event_handle = raw_handle,
                                error = %e
                            );
                        }
                    }

                    unsafe {
                        let _ = EvtClose(event_handle);
                    }
                }
            }

            if channel_drained && !bookmark_failed {
                unsafe {
                    let _ = ResetEvent(channel_sub.signal_event);
                }

                // Update channel record count gauge for lag detection.
                super::render::update_channel_records(
                    &channel_sub.channel,
                    &channel_sub.channel_records_gauge,
                );
            }
        }

        Ok(all_events)
    }

    /// Re-subscribe a channel after its query position becomes invalid
    /// (e.g., an admin cleared the event log). Closes the old subscription
    /// handle and creates a new one using the current bookmark.
    fn resubscribe_channel(
        channel_sub: &mut ChannelSubscription,
        config: &WindowsEventLogConfig,
    ) -> Result<(), WindowsEventLogError> {
        // Close the stale subscription handle
        unsafe {
            let _ = EvtClose(channel_sub.subscription_handle);
        }

        let channel_hstring = HSTRING::from(channel_sub.channel.as_str());
        let query = Self::build_xpath_query(config)?;
        let query_hstring = HSTRING::from(query);

        let bookmark_handle = channel_sub.bookmark.as_handle();
        let has_bookmark = bookmark_handle.0 != 0;

        // Use EvtSubscribeStrict when resuming from bookmark so Windows fails
        // explicitly if the bookmark position is stale, rather than silently
        // falling back to oldest-record.
        let subscription_flags = if has_bookmark {
            EvtSubscribeStartAfterBookmark.0 | EvtSubscribeStrict.0
        } else {
            EvtSubscribeStartAtOldestRecord.0
        };

        let fallback_flags = if config.read_existing_events {
            EvtSubscribeStartAtOldestRecord.0
        } else {
            EvtSubscribeToFutureEvents.0
        };

        let new_handle = unsafe {
            if has_bookmark {
                let strict_result = EvtSubscribe(
                    None,
                    channel_sub.signal_event,
                    &channel_hstring,
                    &query_hstring,
                    bookmark_handle,
                    None,
                    None,
                    subscription_flags,
                );
                match strict_result {
                    Ok(handle) => Ok(handle),
                    Err(e) => {
                        warn!(
                            message = "Strict bookmark resubscribe failed, retrying without bookmark. Potential re-delivery of events.",
                            channel = %channel_sub.channel,
                            error = %e,
                            fallback_flags = format!("{:#x}", fallback_flags)
                        );
                        EvtSubscribe(
                            None,
                            channel_sub.signal_event,
                            &channel_hstring,
                            &query_hstring,
                            None,
                            None,
                            None,
                            fallback_flags,
                        )
                    }
                }
            } else {
                EvtSubscribe(
                    None,
                    channel_sub.signal_event,
                    &channel_hstring,
                    &query_hstring,
                    None,
                    None,
                    None,
                    subscription_flags,
                )
            }
        }
        .map_err(|e| WindowsEventLogError::CreateSubscriptionError { source: e })?;

        channel_sub.subscription_handle = new_handle;
        channel_sub.subscription_active_gauge.set(1.0);

        counter!(
            "windows_event_log_resubscriptions_total",
            "channel" => channel_sub.channel.clone()
        )
        .increment(1);

        Ok(())
    }

    /// Returns the raw shutdown event handle value for use in the async shutdown watcher.
    ///
    /// The returned pointer is the underlying value of the Windows HANDLE. It can be
    /// safely copied and used from another thread to call `SetEvent` because Windows
    /// kernel objects are reference-counted and remain valid as long as at least one
    /// handle is open (which this subscription maintains until Drop).
    pub const fn shutdown_event_raw(&self) -> *mut std::ffi::c_void {
        self.shutdown_event.0
    }

    /// Returns a reference to the rate limiter, if configured.
    pub const fn rate_limiter(
        &self,
    ) -> Option<&RateLimiter<NotKeyed, InMemoryState, DefaultClock>> {
        self.rate_limiter.as_ref()
    }

    /// Returns (total_channels, active_channels) for health reporting.
    pub fn channel_health_summary(&self) -> (usize, usize) {
        let total = self.channels.len();
        // A channel is considered active if its subscription handle is non-null
        let active = self
            .channels
            .iter()
            .filter(|c| c.subscription_handle.0 != 0)
            .count();
        (total, active)
    }

    /// Flush all bookmarks to checkpoint storage.
    ///
    /// Call this before shutdown to ensure no events are lost.
    pub async fn flush_bookmarks(&mut self) -> Result<(), WindowsEventLogError> {
        debug!(message = "Flushing bookmarks to checkpoint storage.");

        let bookmark_xmls: Vec<(String, String)> = self
            .channels
            .iter()
            .filter_map(
                |sub| match BookmarkManager::serialize_handle(sub.bookmark.as_handle()) {
                    Ok(xml) if xml_parser::is_valid_bookmark_xml(&xml) => {
                        Some((sub.channel.clone(), xml))
                    }
                    Ok(_) => None,
                    Err(e) => {
                        emit!(WindowsEventLogBookmarkError {
                            channel: sub.channel.clone(),
                            error: e.to_string(),
                        });
                        None
                    }
                },
            )
            .collect();

        if !bookmark_xmls.is_empty() {
            self.checkpointer.set_batch(bookmark_xmls).await?;
            counter!("windows_event_log_checkpoint_writes_total").increment(1);
        }

        debug!(message = "Bookmark flush complete.");
        Ok(())
    }

    /// Get the current bookmark XML for a specific channel.
    ///
    /// Used for acknowledgment-based checkpointing where the bookmark
    /// state needs to be captured when events are read (not when they're acknowledged).
    pub fn get_bookmark_xml(&self, channel: &str) -> Option<String> {
        self.channels
            .iter()
            .find(|sub| sub.channel == channel)
            .and_then(
                |sub| match BookmarkManager::serialize_handle(sub.bookmark.as_handle()) {
                    Ok(xml) if xml_parser::is_valid_bookmark_xml(&xml) => Some(xml),
                    _ => None,
                },
            )
    }

    fn build_xpath_query(config: &WindowsEventLogConfig) -> Result<String, WindowsEventLogError> {
        build_xpath_query(config)
    }

    fn validate_channels(config: &WindowsEventLogConfig) -> Result<(), WindowsEventLogError> {
        for channel in &config.channels {
            let channel_hstring = HSTRING::from(channel.as_str());
            let channel_handle = unsafe { EvtOpenChannelConfig(None, &channel_hstring, 0) };

            match channel_handle {
                Ok(handle) => {
                    if let Err(e) = unsafe { EvtClose(handle) } {
                        warn!(message = "Failed to close channel config handle.", error = %e);
                    }
                }
                Err(e) => {
                    let error_code = (e.code().0 as u32) & 0xFFFF;
                    if error_code == ERROR_FILE_NOT_FOUND
                        || error_code == ERROR_EVT_CHANNEL_NOT_FOUND
                        || error_code == ERROR_EVT_INVALID_QUERY
                    {
                        // Non-existent channels are skipped during EvtSubscribe below,
                        // so warn here rather than failing the entire source.
                        warn!(
                            message = "Channel not found, will be skipped.",
                            channel = %channel
                        );
                        continue;
                    } else if error_code == ERROR_ACCESS_DENIED {
                        warn!(
                            message = "Channel access denied, will be skipped.",
                            channel = %channel
                        );
                        continue;
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
}

/// Maximum XPath query length supported by Windows Event Log API.
/// Queries exceeding this limit fall back to `"*"` (all events).
const XPATH_MAX_LENGTH: usize = 4096;

/// Build an XPath query from config, incorporating `only_event_ids` when no
/// explicit `event_query` is set.
///
/// When `only_event_ids` is configured and no custom `event_query` is provided,
/// generates a query like `*[System[EventID=4624 or EventID=4625]]` so that
/// the Windows API filters events at the source, avoiding the cost of pulling,
/// rendering, and discarding non-matching events.
///
/// If the generated query exceeds [`XPATH_MAX_LENGTH`] (4096 chars), falls back
/// to `"*"` and lets the downstream filter in `build_event()` handle it.
pub(super) fn build_xpath_query(
    config: &WindowsEventLogConfig,
) -> Result<String, WindowsEventLogError> {
    // Explicit event_query always takes precedence.
    if let Some(ref custom_query) = config.event_query {
        return Ok(custom_query.clone());
    }

    // Generate XPath from only_event_ids if present and non-empty.
    if let Some(ref ids) = config.only_event_ids
        && !ids.is_empty()
    {
        let query = if ids.len() == 1 {
            format!("*[System[EventID={}]]", ids[0])
        } else {
            let predicates: Vec<String> = ids.iter().map(|id| format!("EventID={id}")).collect();
            format!("*[System[{}]]", predicates.join(" or "))
        };

        if query.len() <= XPATH_MAX_LENGTH {
            return Ok(query);
        }
        // Query too long — fall back to wildcard and rely on
        // the in-process filter in build_event().
        warn!(
            message = "Generated XPath query exceeds maximum length, falling back to wildcard.",
            query_len = query.len(),
            max_len = XPATH_MAX_LENGTH,
            num_event_ids = ids.len(),
        );
    }

    Ok("*".to_string())
}

impl Drop for EventLogSubscription {
    fn drop(&mut self) {
        // Close subscription handles and signal events
        for sub in &self.channels {
            unsafe {
                let _ = EvtClose(sub.subscription_handle);
                let _ = CloseHandle(sub.signal_event);
            }
        }
        // Publisher metadata handles are closed automatically by PublisherHandle::drop
        // when the LRU cache is dropped.

        // Close shutdown event
        unsafe {
            let _ = CloseHandle(self.shutdown_event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_checkpointer() -> (Arc<Checkpointer>, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let checkpointer = Arc::new(Checkpointer::new(temp_dir.path()).await.unwrap());
        (checkpointer, temp_dir)
    }

    #[test]
    fn test_rate_limiter_configuration() {
        let mut config = WindowsEventLogConfig::default();
        assert_eq!(config.events_per_second, 0);

        config.events_per_second = 1000;
        assert_eq!(config.events_per_second, 1000);
    }

    #[tokio::test]
    async fn test_rate_limiter_disabled_by_default() {
        let config = WindowsEventLogConfig::default();
        assert_eq!(
            config.events_per_second, 0,
            "Rate limiting should be disabled by default"
        );
    }

    /// Test pull subscription creation and basic operation
    #[tokio::test]
    async fn test_pull_subscription_creation() {
        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.event_timeout_ms = 1000;

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let subscription = EventLogSubscription::new(&config, checkpointer, false).await;
        assert!(
            subscription.is_ok(),
            "Pull subscription creation should succeed: {:?}",
            subscription.err()
        );

        let sub = subscription.unwrap();
        assert_eq!(
            sub.channels.len(),
            1,
            "Should have one channel subscription"
        );
    }

    /// Test that wait_for_events_blocking returns timeout or events available
    #[tokio::test]
    async fn test_wait_for_events_timeout() {
        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.read_existing_events = false;
        config.event_timeout_ms = 100;

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let subscription = EventLogSubscription::new(&config, checkpointer, false)
            .await
            .expect("Subscription creation should succeed");

        // Use ownership transfer pattern for spawn_blocking
        let (subscription, result) = tokio::task::spawn_blocking(move || {
            let r = subscription.wait_for_events_blocking(100);
            (subscription, r)
        })
        .await
        .unwrap();

        // The first call may return EventsAvailable since signals are initially signaled.
        // That's expected behavior per the pull model design.
        match result {
            WaitResult::EventsAvailable | WaitResult::Timeout => {}
            WaitResult::Shutdown => panic!("Should not get shutdown"),
        }

        // Keep subscription alive until end of test
        drop(subscription);
    }

    /// Test that signal_shutdown wakes a waiting thread
    #[tokio::test]
    async fn test_shutdown_signal_wakes_wait() {
        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.event_timeout_ms = 500;

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let subscription = EventLogSubscription::new(&config, checkpointer, false)
            .await
            .expect("Subscription creation should succeed");

        // First drain the initially-signaled state using ownership transfer
        let (subscription, _) = tokio::task::spawn_blocking(move || {
            let r = subscription.wait_for_events_blocking(50);
            (subscription, r)
        })
        .await
        .unwrap();

        let shutdown_event_raw = subscription.shutdown_event_raw() as isize;

        let wait_handle = tokio::task::spawn_blocking(move || {
            let r = subscription.wait_for_events_blocking(30000);
            (subscription, r)
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        unsafe {
            let handle = HANDLE(shutdown_event_raw as *mut std::ffi::c_void);
            let _ = SetEvent(handle);
        }

        let (subscription, result) = wait_handle.await.unwrap();
        match result {
            WaitResult::Shutdown => {} // Expected
            WaitResult::EventsAvailable => {
                // Acceptable - there may have been real events
            }
            WaitResult::Timeout => {
                panic!("Should not timeout - shutdown should have woken the wait");
            }
        }

        drop(subscription);
    }

    /// Test pull_events with read_existing_events=true
    #[tokio::test]
    async fn test_pull_events_returns_events() {
        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.read_existing_events = true;
        config.event_timeout_ms = 2000;

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;

        let subscription = EventLogSubscription::new(&config, checkpointer, false)
            .await
            .expect("Subscription creation should succeed");

        // Wait and pull using ownership transfer pattern
        let (mut subscription, wait_result) = tokio::task::spawn_blocking(move || {
            let r = subscription.wait_for_events_blocking(2000);
            (subscription, r)
        })
        .await
        .unwrap();

        match wait_result {
            WaitResult::EventsAvailable => {
                let events = subscription.pull_events(100).unwrap();
                assert!(
                    !events.is_empty(),
                    "With read_existing_events=true, should get historical events"
                );
            }
            WaitResult::Timeout => {
                // Might happen on a system with empty Application log
            }
            WaitResult::Shutdown => panic!("Unexpected shutdown"),
        }
    }

    /// Test multiple concurrent pull subscriptions
    #[tokio::test]
    async fn test_multiple_concurrent_subscriptions() {
        let mut config1 = WindowsEventLogConfig::default();
        config1.channels = vec!["Application".to_string()];
        config1.event_timeout_ms = 1000;

        let mut config2 = WindowsEventLogConfig::default();
        config2.channels = vec!["System".to_string()];
        config2.event_timeout_ms = 1000;

        let (checkpointer1, _temp_dir1) = create_test_checkpointer().await;
        let (checkpointer2, _temp_dir2) = create_test_checkpointer().await;

        let sub1 = EventLogSubscription::new(&config1, checkpointer1, false)
            .await
            .expect("Subscription 1 (Application) should succeed");
        let sub2 = EventLogSubscription::new(&config2, checkpointer2, false)
            .await
            .expect("Subscription 2 (System) should succeed");

        // Both should be independently functional
        assert_eq!(sub1.channels.len(), 1);
        assert_eq!(sub2.channels.len(), 1);
        assert_eq!(sub1.channels[0].channel, "Application");
        assert_eq!(sub2.channels[0].channel, "System");
    }

    /// Test read_existing_events=false only receives future events
    #[tokio::test]
    async fn test_read_existing_events_false_only_receives_future_events() {
        use chrono::Utc;

        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.read_existing_events = false;
        config.event_timeout_ms = 500;

        let (checkpointer, _temp_dir) = create_test_checkpointer().await;
        let subscription_start_time = Utc::now();

        let mut subscription = EventLogSubscription::new(&config, checkpointer, false)
            .await
            .expect("Subscription creation should succeed");

        // Brief wait then pull
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let events = subscription.pull_events(100).unwrap_or_default();

        let tolerance = chrono::Duration::seconds(5);
        let earliest_allowed = subscription_start_time - tolerance;

        for event in &events {
            assert!(
                event.time_created >= earliest_allowed,
                "Event timestamp {} is before subscription start time {} (minus tolerance). \
                 read_existing_events=false may not be respected. Event ID: {}, Record ID: {}",
                event.time_created,
                subscription_start_time,
                event.event_id,
                event.record_id
            );
        }
    }

    /// Test that subscription gracefully handles an invalid/corrupted bookmark
    /// from a checkpoint, falling back to a fresh bookmark without crashing.
    #[tokio::test]
    async fn test_checkpoint_with_invalid_bookmark_falls_back_gracefully() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let checkpointer = Arc::new(Checkpointer::new(temp_dir.path()).await.unwrap());

        let fake_bookmark = r#"<BookmarkList><Bookmark Channel='Application' RecordId='999999999' IsCurrent='true'/></BookmarkList>"#;

        checkpointer
            .set("Application".to_string(), fake_bookmark.to_string())
            .await
            .expect("Should be able to set checkpoint");

        let mut config = WindowsEventLogConfig::default();
        config.channels = vec!["Application".to_string()];
        config.read_existing_events = true;
        config.event_timeout_ms = 500;

        // The subscription should succeed even with a corrupted/invalid bookmark,
        // gracefully falling back to a fresh bookmark.
        let mut subscription = EventLogSubscription::new(&config, checkpointer, false)
            .await
            .expect("Subscription should succeed even with invalid bookmark checkpoint");

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Just verify we can pull events without panicking.
        // The bookmark format above is not a real Windows bookmark, so the
        // subscription will fall back to reading from scratch. We only assert
        // that the subscription is functional.
        let _events = subscription.pull_events(100).unwrap_or_default();
    }
}
