use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt, StreamExt,
};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, error, trace};

use crate::FilePosition;

/// Represents the state of a file being watched by the notify-based watcher
#[derive(Debug)]
struct FileState {
    /// Path to the file being watched
    path: PathBuf,
}

/// A watcher implementation that uses notify-rs/notify for filesystem notifications
/// instead of polling. This allows for more efficient file watching, especially
/// for files that are not frequently updated.
pub struct NotifyWatcher {
    /// The underlying notify watcher
    watcher: Option<RecommendedWatcher>,
    /// Channel for receiving events from the watcher
    event_rx: Option<Receiver<Result<Event, notify::Error>>>,
    /// Paths of all files being watched
    watched_files: Arc<Mutex<Vec<FileState>>>,
    /// Async mutex for thread-safe access to the event receiver
    event_mutex: Arc<TokioMutex<()>>,
}

impl NotifyWatcher {
    /// Create a new NotifyWatcher
    pub fn new() -> Self {
        NotifyWatcher {
            watcher: None,
            event_rx: None,
            watched_files: Arc::new(Mutex::new(Vec::new())),
            event_mutex: Arc::new(TokioMutex::new(())),
        }
    }

    /// Create an async watcher that uses futures channels
    async fn async_watcher() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)>
    {
        let (tx, rx) = channel(100); // Use a larger buffer to avoid missing events

        // Create a watcher with a callback that sends events to the channel
        let watcher = RecommendedWatcher::new(
            move |res| {
                // Check if we're shutting down
                static SHUTTING_DOWN: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if SHUTTING_DOWN.load(std::sync::atomic::Ordering::SeqCst) {
                    // Skip sending events during shutdown
                    return;
                }

                // Use a synchronous channel send to avoid requiring tokio runtime
                // This is necessary because the notify-rs callback runs in its own thread
                // outside of the tokio runtime
                let mut tx = tx.clone();
                // Use a blocking executor to send the event
                // This avoids the need for a tokio runtime
                futures::executor::block_on(async {
                    if let Err(e) = tx.send(res).await {
                        // Only log at trace level to avoid spamming the logs during shutdown
                        trace!(message = "Failed to send event to channel", error = ?e);
                    }
                });
            },
            Config::default()
                // Use a very short polling interval as fallback to minimize delays
                .with_poll_interval(std::time::Duration::from_millis(100))
                // We only care about file modifications, creations, and renames
                .with_compare_contents(false),
        )?;

        Ok((watcher, rx))
    }

    /// Initialize the watcher with a specific path
    pub async fn initialize(&mut self, path: &Path) -> Result<(), notify::Error> {
        // Create an async watcher
        let (mut watcher, rx) = Self::async_watcher().await?;

        // Watch the parent directory of the file to catch renames, deletions, etc.
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        watcher.watch(parent, RecursiveMode::NonRecursive)?;

        debug!(message = "Initialized async notify watcher for directory", directory = ?parent, target_file = ?path);

        self.watcher = Some(watcher);
        self.event_rx = Some(rx);

        Ok(())
    }

    /// Add a file to be watched
    pub async fn watch_file(
        &mut self,
        path: PathBuf,
        _file_position: FilePosition, // We don't need to store the file position anymore
    ) -> Result<(), notify::Error> {
        if self.watcher.is_none() {
            self.initialize(&path).await?
        }

        // Check if we're already watching this file to avoid duplicates
        let mut files = self.watched_files.lock().unwrap();
        if !files.iter().any(|state| state.path == path) {
            let state = FileState { path };
            files.push(state);
        }

        Ok(())
    }

    /// Check for any events on the watched files
    ///
    /// Only returns events that indicate actual file changes (writes, moves, or renames)
    /// to avoid reacting to our own file accesses.
    ///
    /// This method will try to receive events immediately using the async channel.
    pub async fn check_events(&mut self) -> Vec<(PathBuf, EventKind)> {
        let mut events = Vec::new();

        // Use a mutex to ensure only one thread is checking events at a time
        let _lock = self.event_mutex.lock().await;

        if let Some(ref mut rx) = self.event_rx {
            // Try to receive events with a timeout
            let timeout =
                tokio::time::timeout(std::time::Duration::from_millis(10), rx.next()).await;

            match timeout {
                Ok(Some(Ok(event))) => {
                    // Don't log Access events or Other events at all to eliminate noise
                    match event.kind {
                        EventKind::Access(_) => {
                            // Skip logging for Access events
                        }
                        EventKind::Other => {
                            // Skip logging for all Other events
                        }
                        _ => {
                            debug!(message = "Received file event", ?event);
                        }
                    }

                    // Filter for relevant events only
                    let is_relevant = match event.kind {
                        // File content was modified
                        EventKind::Modify(notify::event::ModifyKind::Data(_)) => true,
                        // File was created or moved
                        EventKind::Create(_) => true,
                        // File was renamed
                        EventKind::Modify(notify::event::ModifyKind::Name(_)) => true,
                        // Explicitly filter out Access events (our own file opens)
                        EventKind::Access(_) => false,
                        // Explicitly filter out all Other events
                        EventKind::Other => false,
                        // Other events are not relevant for our purposes
                        _ => {
                            trace!(message = "Ignoring other event type", kind = ?event.kind);
                            false
                        }
                    };

                    if is_relevant {
                        for path in event.paths {
                            // Check if this path is one of our watched files
                            let is_watched = {
                                let files = self.watched_files.lock().unwrap();
                                files.iter().any(|state| state.path == path)
                            };

                            if is_watched {
                                debug!(message = "Relevant file event detected for watched file", ?path, kind = ?event.kind);
                                events.push((path, event.kind));
                            } else {
                                trace!(message = "Ignoring event for unwatched file", ?path);
                            }
                        }
                    } else {
                        trace!(message = "Ignoring non-relevant file event", kind = ?event.kind);
                    }
                }
                Ok(Some(Err(e))) => {
                    error!(message = "Error receiving file event", error = ?e);
                }
                Ok(None) => {
                    // Channel closed
                    error!(message = "Notify watcher channel closed");
                }
                Err(_) => {
                    // Timeout occurred, no events available
                    trace!(message = "No events received within timeout");
                }
            }
        }

        events
    }

    // Note: The methods activate, deactivate, get_file_position, and update_file_position
    // have been removed as they were not used in the codebase.

    /// Shutdown the watcher
    ///
    /// This method should be called when the watcher is no longer needed,
    /// such as when Vector is shutting down. It drops the watcher and
    /// channel to prevent further events from being sent.
    pub fn shutdown(&mut self) {
        // First, set a flag to indicate we're shutting down
        // This is used in the callback to avoid sending events during shutdown
        static SHUTTING_DOWN: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::SeqCst);

        // Drop the watcher to stop receiving events
        self.watcher = None;
        // Drop the channel to prevent further events from being sent
        self.event_rx = None;

        debug!(message = "NotifyWatcher shut down");
    }
}
