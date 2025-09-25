use std::collections::HashSet;
use std::path::{Path, PathBuf};

use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt, StreamExt,
};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
// use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, error, trace};

use crate::FilePosition;

/// A watcher implementation that uses notify-rs/notify for filesystem notifications
/// instead of polling. This allows for more efficient file watching, especially
/// for files that are not frequently updated.
pub struct NotifyWatcher {
    /// The underlying notify watcher
    watcher: Option<RecommendedWatcher>,
    /// Channel for receiving events from the watcher
    event_rx: Option<Receiver<Result<Event, notify::Error>>>,
    /// Paths of all files being watched
    watched_files: HashSet<PathBuf>,
}

impl NotifyWatcher {
    /// Create a new NotifyWatcher
    pub fn new() -> Self {
        NotifyWatcher {
            watcher: None,
            event_rx: None,
            watched_files: HashSet::new(),
        }
    }

    /// Create an async watcher that uses futures channels
    async fn async_watcher() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)>
    {
        let (tx, rx) = channel(100); // Use a larger buffer to avoid missing events

        let rt = tokio::runtime::Handle::current();

        // Create a watcher with a callback that sends events to the channel
        let watcher = RecommendedWatcher::new(
            move |res| {
                // FIXME this is totally incorrect
                // // Check if we're shutting down
                // static SHUTTING_DOWN: std::sync::atomic::AtomicBool =
                //     std::sync::atomic::AtomicBool::new(false);
                // if SHUTTING_DOWN.load(std::sync::atomic::Ordering::SeqCst) {
                //     // Skip sending events during shutdown
                //     return;
                // }

                // Use a synchronous channel send to avoid requiring tokio runtime
                // This is necessary because the notify-rs callback runs in its own thread
                // outside of the tokio runtime
                let mut tx = tx.clone();
                // Use a blocking executor to send the event
                // This avoids the need for a tokio runtime
                rt.block_on(async {
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

        // Insert to set, duplciated automatically ignored
        self.watched_files.insert(path);

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

        let Some(ref mut rx) = self.event_rx else {
            return events;
        };

        // Try to receive events with a timeout
        let timeout = tokio::time::timeout(std::time::Duration::from_millis(10), rx.next()).await;

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
                        let is_watched = self.watched_files.contains(&path);

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

        events
    }

    pub fn shutdown(&mut self) {
        // Drop the watcher to stop receiving events
        self.watcher = None;
        // Drop the channel to prevent further events from being sent
        self.event_rx = None;

        debug!(message = "NotifyWatcher shut down");
    }
}
