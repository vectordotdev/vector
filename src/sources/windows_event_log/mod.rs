use async_trait::async_trait;
use vector_lib::config::LogNamespace;
use vrl::value::{Kind, kind::Collection};

use vector_config::component::SourceDescription;

use crate::config::{DataType, SourceConfig, SourceContext, SourceOutput};

// Cross-platform: config types (pure serde structs, no Windows dependencies)
mod config;
pub use self::config::*;

cfg_if::cfg_if! {
    if #[cfg(windows)] {
        mod bookmark;
        mod checkpoint;
        pub mod error;
        mod metadata;
        mod parser;
        mod render;
        mod sid_resolver;
        mod subscription;
        mod xml_parser;

        use std::path::PathBuf;
        use std::sync::Arc;

        use futures::StreamExt;
        use vector_lib::EstimatedJsonEncodedSizeOf;
        use vector_lib::finalizer::OrderedFinalizer;
        use vector_lib::internal_event::{
            ByteSize, BytesReceived, CountByteSize, InternalEventHandle as _, Protocol,
        };
        use windows::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
        use windows::Win32::System::Threading::GetCurrentProcess;

        use crate::{
            SourceSender,
            event::{BatchNotifier, BatchStatus, BatchStatusReceiver},
            internal_events::{
                EventsReceived, StreamClosedError, WindowsEventLogParseError, WindowsEventLogQueryError,
            },
            shutdown::ShutdownSignal,
        };

        use self::{
            checkpoint::Checkpointer,
            error::WindowsEventLogError,
            parser::EventLogParser,
            subscription::{EventLogSubscription, WaitResult},
        };
    }
}

#[cfg(all(test, windows))]
mod tests;

// Integration tests are feature-gated to avoid requiring Windows Event Log service.
// To run integration tests on Windows: cargo test --features sources-windows_event_log-integration-tests
#[cfg(all(test, windows, feature = "sources-windows_event_log-integration-tests"))]
mod integration_tests;

cfg_if::cfg_if! {
if #[cfg(windows)] {

/// Entry for the acknowledgment finalizer containing checkpoint information.
/// Each entry represents a batch of events that need to be acknowledged before
/// the checkpoint can be safely updated. Contains all channel bookmarks from
/// the batch since a single batch may span multiple channels.
#[derive(Debug, Clone)]
struct FinalizerEntry {
    /// Channel bookmarks: (channel_name, bookmark_xml) pairs
    bookmarks: Vec<(String, String)>,
}

/// Shared checkpointer type for use with the finalizer
type SharedCheckpointer = Arc<Checkpointer>;

/// Finalizer for handling acknowledgments.
/// Supports both synchronous (immediate checkpoint) and asynchronous (deferred checkpoint) modes.
enum Finalizer {
    /// Synchronous mode: checkpoints are updated immediately after reading events.
    /// Used when acknowledgements are disabled.
    Sync(SharedCheckpointer),
    /// Asynchronous mode: checkpoints are updated only after downstream sinks acknowledge receipt.
    /// Used when acknowledgements are enabled.
    Async(OrderedFinalizer<FinalizerEntry>),
}

impl Finalizer {
    /// Create a new finalizer based on acknowledgement configuration.
    fn new(
        acknowledgements: bool,
        checkpointer: SharedCheckpointer,
        shutdown: ShutdownSignal,
    ) -> Self {
        if acknowledgements {
            let (finalizer, mut ack_stream) =
                OrderedFinalizer::<FinalizerEntry>::new(Some(shutdown.clone()));

            // Spawn background task to process acknowledgments and update checkpoints
            tokio::spawn(async move {
                while let Some((status, entry)) = ack_stream.next().await {
                    if status == BatchStatus::Delivered {
                        if let Err(e) = checkpointer.set_batch(entry.bookmarks.clone()).await {
                            warn!(
                                message = "Failed to update checkpoint after acknowledgement.",
                                error = %e
                            );
                        } else {
                            debug!(
                                message = "Checkpoint updated after acknowledgement.",
                                channels = entry.bookmarks.len()
                            );
                        }
                    } else {
                        debug!(
                            message = "Events not delivered, checkpoint not updated.",
                            status = ?status
                        );
                    }
                }
                debug!(message = "Acknowledgement stream completed.");
            });

            Self::Async(finalizer)
        } else {
            Self::Sync(checkpointer)
        }
    }

    /// Finalize a batch of events.
    /// In sync mode, immediately updates the checkpoint.
    /// In async mode, registers the entry for deferred checkpoint update.
    async fn finalize(&self, entry: FinalizerEntry, receiver: Option<BatchStatusReceiver>) {
        match (self, receiver) {
            (Self::Sync(checkpointer), None) => {
                if let Err(e) = checkpointer.set_batch(entry.bookmarks.clone()).await {
                    warn!(
                        message = "Failed to update checkpoint.",
                        error = %e
                    );
                }
            }
            (Self::Async(finalizer), Some(receiver)) => {
                finalizer.add(entry, receiver);
            }
            (Self::Sync(_), Some(_)) => {
                warn!(message = "Received acknowledgement receiver in sync mode, ignoring.");
            }
            (Self::Async(_), None) => {
                warn!(
                    message = "No acknowledgement receiver in async mode, checkpoint may be lost."
                );
            }
        }
    }
}

/// Windows Event Log source implementation
pub struct WindowsEventLogSource {
    config: WindowsEventLogConfig,
    data_dir: PathBuf,
    acknowledgements: bool,
    log_namespace: LogNamespace,
}

impl WindowsEventLogSource {
    pub fn new(
        config: WindowsEventLogConfig,
        data_dir: PathBuf,
        acknowledgements: bool,
        log_namespace: LogNamespace,
    ) -> crate::Result<Self> {
        config.validate()?;

        Ok(Self {
            config,
            data_dir,
            acknowledgements,
            log_namespace,
        })
    }

    async fn run_internal(
        &mut self,
        mut out: SourceSender,
        shutdown: ShutdownSignal,
    ) -> Result<(), WindowsEventLogError> {
        let checkpointer = Arc::new(Checkpointer::new(&self.data_dir).await?);

        let finalizer = Finalizer::new(
            self.acknowledgements,
            Arc::clone(&checkpointer),
            shutdown.clone(),
        );

        let mut subscription = EventLogSubscription::new(
            &self.config,
            Arc::clone(&checkpointer),
            self.acknowledgements,
        )
        .await?;
        let parser = EventLogParser::new(&self.config, self.log_namespace);

        let events_received = register!(EventsReceived);
        let bytes_received = register!(BytesReceived::from(Protocol::from("windows_event_log")));

        let timeout_ms = self.config.event_timeout_ms as u32;
        let batch_size = self.config.batch_size as usize;
        let acknowledgements = self.acknowledgements;

        info!(
            message = "Starting Windows Event Log source (pull mode).",
            acknowledgements = acknowledgements,
        );

        // Spawn async shutdown watcher that signals the Windows shutdown event
        // when the Vector shutdown signal fires. This wakes WaitForMultipleObjects
        // while subscription is moved into spawn_blocking.
        //
        // We duplicate the handle so the watcher owns an independent kernel reference.
        // This prevents use-after-close if the subscription panics and drops before
        // the watcher fires — the duplicate remains valid until explicitly closed.
        let (watcher_handle_raw, watcher_owns_handle): (isize, bool) = {
            unsafe {
                let src = HANDLE(subscription.shutdown_event_raw());
                let process = GetCurrentProcess();
                let mut dup = HANDLE::default();
                if DuplicateHandle(
                    process,
                    src,
                    process,
                    &mut dup,
                    0,
                    false,
                    DUPLICATE_SAME_ACCESS,
                )
                .is_ok()
                {
                    (dup.0 as isize, true)
                } else {
                    // Fallback: use the original handle without ownership.
                    // The watcher will signal but NOT close — EventLogSubscription::drop
                    // owns the handle and will close it.
                    warn!(
                        message = "Failed to duplicate shutdown event handle, falling back to shared handle."
                    );
                    (src.0 as isize, false)
                }
            }
        };
        let shutdown_watcher = shutdown.clone();
        tokio::spawn(async move {
            shutdown_watcher.await;
            unsafe {
                let handle =
                    windows::Win32::Foundation::HANDLE(watcher_handle_raw as *mut std::ffi::c_void);
                let _ = windows::Win32::System::Threading::SetEvent(handle);
                if watcher_owns_handle {
                    let _ = windows::Win32::Foundation::CloseHandle(handle);
                }
            }
        });

        // Track when we last flushed checkpoints
        let mut last_checkpoint = std::time::Instant::now();
        let checkpoint_interval =
            std::time::Duration::from_secs(self.config.checkpoint_interval_secs);

        // Exponential backoff on consecutive recoverable errors
        let mut error_backoff = std::time::Duration::from_millis(100);
        const MAX_ERROR_BACKOFF: std::time::Duration = std::time::Duration::from_secs(5);

        // Health heartbeat: log every ~30s regardless of checkpoint interval
        let mut timeout_count: u32 = 0;
        let health_interval_timeouts = (30_000 / self.config.event_timeout_ms).max(1) as u32;

        loop {
            // Move subscription into blocking thread for WaitForMultipleObjects.
            // Ownership transfer ensures no data races between the blocking thread
            // and async code. The shutdown watcher uses a raw HANDLE value (just an
            // integer) to signal shutdown without needing access to the subscription.
            let (returned_sub, wait_result) = tokio::task::spawn_blocking({
                let sub = subscription;
                move || {
                    let result = sub.wait_for_events_blocking(timeout_ms);
                    (sub, result)
                }
            })
            .await
            .map_err(|e| WindowsEventLogError::ConfigError {
                message: format!("Wait task panicked: {e}"),
            })?;

            subscription = returned_sub;

            match wait_result {
                WaitResult::EventsAvailable => {
                    // Pull events via spawn_blocking (EvtNext/EvtRender are blocking APIs)
                    let (returned_sub, events_result) = tokio::task::spawn_blocking({
                        let mut sub = subscription;
                        move || {
                            let result = sub.pull_events(batch_size);
                            (sub, result)
                        }
                    })
                    .await
                    .map_err(|e| WindowsEventLogError::ConfigError {
                        message: format!("Pull task panicked: {e}"),
                    })?;

                    subscription = returned_sub;

                    // Rate limiting between batches (async-compatible)
                    if let Some(limiter) = subscription.rate_limiter() {
                        limiter.until_ready().await;
                    }

                    match events_result {
                        Ok(events) if events.is_empty() => {
                            error_backoff = std::time::Duration::from_millis(100);
                            continue;
                        }
                        Ok(events) => {
                            error_backoff = std::time::Duration::from_millis(100);
                            debug!(
                                message = "Pulled Windows Event Log events.",
                                event_count = events.len()
                            );

                            let (batch, receiver) =
                                BatchNotifier::maybe_new_with_receiver(acknowledgements);

                            let mut log_events = Vec::new();
                            let mut total_byte_size = 0;
                            let mut channels_in_batch = std::collections::HashSet::new();

                            for event in events {
                                let channel = event.channel.clone();
                                channels_in_batch.insert(channel.clone());
                                let event_id = event.event_id;
                                match parser.parse_event(event) {
                                    Ok(mut log_event) => {
                                        let byte_size = log_event.estimated_json_encoded_size_of();
                                        total_byte_size += byte_size.get();

                                        if let Some(ref batch) = batch {
                                            log_event = log_event.with_batch_notifier(batch);
                                        }

                                        log_events.push(log_event);
                                    }
                                    Err(e) => {
                                        emit!(WindowsEventLogParseError {
                                            error: e.to_string(),
                                            channel,
                                            event_id: Some(event_id),
                                        });
                                    }
                                }
                            }

                            if !log_events.is_empty() {
                                let count = log_events.len();
                                events_received.emit(CountByteSize(count, total_byte_size.into()));
                                bytes_received.emit(ByteSize(total_byte_size));

                                // BACK PRESSURE: block here until the pipeline accepts
                                // the batch. We don't call EvtNext again until this completes.
                                if let Err(_error) = out.send_batch(log_events).await {
                                    emit!(StreamClosedError { count });
                                    break;
                                }

                                // Register checkpoint entry with finalizer
                                let bookmarks: Vec<(String, String)> = channels_in_batch
                                    .into_iter()
                                    .filter_map(|channel| {
                                        subscription
                                            .get_bookmark_xml(&channel)
                                            .map(|xml| (channel, xml))
                                    })
                                    .collect();

                                if !bookmarks.is_empty() {
                                    let entry = FinalizerEntry { bookmarks };
                                    finalizer.finalize(entry, receiver).await;
                                }
                            }
                        }
                        Err(e) => {
                            emit!(WindowsEventLogQueryError {
                                channel: "all".to_string(),
                                query: None,
                                error: e.to_string(),
                            });
                            if !e.is_recoverable() {
                                error!(
                                    message = "Non-recoverable pull error, shutting down.",
                                    error = %e
                                );
                                break;
                            }
                            // Exponential backoff on consecutive recoverable errors
                            warn!(
                                message = "Recoverable pull error, backing off.",
                                backoff_ms = error_backoff.as_millis() as u64,
                                error = %e
                            );
                            tokio::time::sleep(error_backoff).await;
                            error_backoff = (error_backoff * 2).min(MAX_ERROR_BACKOFF);
                        }
                    }
                }

                WaitResult::Timeout => {
                    // A full wait cycle without errors means the system is healthy;
                    // reset backoff so the next transient error starts fresh.
                    error_backoff = std::time::Duration::from_millis(100);

                    // Periodic checkpoint flush (sync mode only)
                    if !acknowledgements && last_checkpoint.elapsed() >= checkpoint_interval {
                        if let Err(e) = subscription.flush_bookmarks().await {
                            warn!(
                                message = "Failed to flush bookmarks during periodic checkpoint.",
                                error = %e
                            );
                        }
                        last_checkpoint = std::time::Instant::now();
                    }

                    // Health heartbeat on a separate ~30s cadence
                    timeout_count += 1;
                    if timeout_count >= health_interval_timeouts {
                        timeout_count = 0;
                        let (total, active) = subscription.channel_health_summary();
                        if active < total {
                            warn!(
                                message = "Some channel subscriptions are inactive.",
                                total_channels = total,
                                active_channels = active,
                            );
                        } else {
                            debug!(
                                message = "All channel subscriptions healthy.",
                                total_channels = total,
                            );
                        }
                    }
                }

                WaitResult::Shutdown => {
                    info!(message = "Windows Event Log wait received shutdown signal.");
                    if !acknowledgements {
                        info!(message = "Flushing bookmarks before shutdown.");
                        if let Err(e) = subscription.flush_bookmarks().await {
                            warn!(message = "Failed to flush bookmarks on shutdown.", error = %e);
                        }
                    }
                    break;
                }
            }
        }

        Ok(())
    }
}

} // if #[cfg(windows)]
} // cfg_if!

#[async_trait]
#[typetag::serde(name = "windows_event_log")]
impl SourceConfig for WindowsEventLogConfig {
    async fn build(&self, _cx: SourceContext) -> crate::Result<super::Source> {
        #[cfg(not(windows))]
        {
            Err("The windows_event_log source is only supported on Windows.".into())
        }

        #[cfg(windows)]
        {
            let data_dir = _cx
                .globals
                .resolve_and_make_data_subdir(self.data_dir.as_ref(), _cx.key.id())?;

            let acknowledgements = _cx.do_acknowledgements(self.acknowledgements);

            let log_namespace = _cx.log_namespace(self.log_namespace);
            let source = WindowsEventLogSource::new(
                self.clone(),
                data_dir,
                acknowledgements,
                log_namespace,
            )?;
            Ok(Box::pin(async move {
                let mut source = source;
                if let Err(error) = source.run_internal(_cx.out, _cx.shutdown).await {
                    error!(message = "Windows Event Log source failed.", %error);
                }
                Ok(())
            }))
        }
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let log_namespace = self
            .log_namespace
            .map(|b| {
                if b {
                    LogNamespace::Vector
                } else {
                    LogNamespace::Legacy
                }
            })
            .unwrap_or(global_log_namespace);

        let schema_definition = match log_namespace {
            LogNamespace::Vector => vector_lib::schema::Definition::new_with_default_metadata(
                Kind::object(std::collections::BTreeMap::from([
                    ("timestamp".into(), Kind::timestamp().or_undefined()),
                    ("message".into(), Kind::bytes().or_undefined()),
                    ("level".into(), Kind::bytes().or_undefined()),
                    ("source".into(), Kind::bytes().or_undefined()),
                    ("event_id".into(), Kind::integer().or_undefined()),
                    ("provider_name".into(), Kind::bytes().or_undefined()),
                    ("computer".into(), Kind::bytes().or_undefined()),
                    ("user_id".into(), Kind::bytes().or_undefined()),
                    ("user_name".into(), Kind::bytes().or_undefined()),
                    ("record_id".into(), Kind::integer().or_undefined()),
                    ("activity_id".into(), Kind::bytes().or_undefined()),
                    ("related_activity_id".into(), Kind::bytes().or_undefined()),
                    ("process_id".into(), Kind::integer().or_undefined()),
                    ("thread_id".into(), Kind::integer().or_undefined()),
                    ("channel".into(), Kind::bytes().or_undefined()),
                    ("opcode".into(), Kind::integer().or_undefined()),
                    ("task".into(), Kind::integer().or_undefined()),
                    ("keywords".into(), Kind::bytes().or_undefined()),
                    ("level_value".into(), Kind::integer().or_undefined()),
                    ("provider_guid".into(), Kind::bytes().or_undefined()),
                    ("version".into(), Kind::integer().or_undefined()),
                    ("qualifiers".into(), Kind::integer().or_undefined()),
                    (
                        "string_inserts".into(),
                        Kind::array(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    ),
                    (
                        "event_data".into(),
                        Kind::object(std::collections::BTreeMap::new()).or_undefined(),
                    ),
                    (
                        "user_data".into(),
                        Kind::object(std::collections::BTreeMap::new()).or_undefined(),
                    ),
                    ("task_name".into(), Kind::bytes().or_undefined()),
                    ("opcode_name".into(), Kind::bytes().or_undefined()),
                    (
                        "keyword_names".into(),
                        Kind::array(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    ),
                ])),
                [LogNamespace::Vector],
            ),
            LogNamespace::Legacy => vector_lib::schema::Definition::any(),
        };

        vec![SourceOutput::new_maybe_logs(
            DataType::Log,
            schema_definition,
        )]
    }

    fn resources(&self) -> Vec<crate::config::Resource> {
        self.channels
            .iter()
            .map(|channel| crate::config::Resource::DiskBuffer(channel.clone()))
            .collect()
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

inventory::submit! {
    SourceDescription::new::<WindowsEventLogConfig>(
        "windows_event_log",
        "Collect logs from Windows Event Log channels",
        "A Windows-specific source that subscribes to Windows Event Log channels and streams events in real-time using the Windows Event Log API.",
        "https://vector.dev/docs/reference/configuration/sources/windows_event_log/"
    )
}
