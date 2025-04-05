use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use notify::{Event, EventKind, RecursiveMode, Watcher};
use tracing::{error, trace};

use crate::FilePosition;

/// Represents the state of a file being watched by the notify-based watcher
#[derive(Debug)]
#[allow(dead_code)]
pub struct NotifyWatcherState {
    /// Path to the file being watched
    pub path: PathBuf,
    /// Last known position in the file
    pub file_position: FilePosition,
    /// Last time the file was seen
    pub last_seen: Instant,
    /// Whether the file is currently being actively watched
    pub is_active: bool,
}

/// A watcher implementation that uses notify-rs/notify for filesystem notifications
/// instead of polling. This allows for more efficient file watching, especially
/// for files that are not frequently updated.
pub struct NotifyWatcher {
    /// The underlying notify watcher
    watcher: Option<Box<dyn Watcher>>,
    /// Channel for receiving events from the watcher
    event_rx: Option<std::sync::mpsc::Receiver<Result<Event, notify::Error>>>,
    /// States of all files being watched
    states: Arc<Mutex<Vec<NotifyWatcherState>>>,
}

impl NotifyWatcher {
    /// Create a new NotifyWatcher
    pub fn new() -> Result<Self, notify::Error> {
        Ok(NotifyWatcher {
            watcher: None,
            event_rx: None,
            states: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Initialize the watcher with a specific path
    pub fn initialize(&mut self, path: &Path) -> Result<(), notify::Error> {
        let (tx, rx) = std::sync::mpsc::channel();

        // Create a custom config that's optimized for our use case
        let config = notify::Config::default()
            // Use a reasonable polling interval as fallback
            .with_poll_interval(std::time::Duration::from_secs(2))
            // We only care about file modifications, creations, and renames
            .with_compare_contents(false);

        // Create the watcher with our custom config
        let mut watcher = notify::RecommendedWatcher::new(tx, config)?;

        // Watch the parent directory of the file to catch renames, deletions, etc.
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        watcher.watch(parent, RecursiveMode::NonRecursive)?;

        trace!(message = "Initialized notify watcher for directory", directory = ?parent, target_file = ?path);

        self.watcher = Some(Box::new(watcher));
        self.event_rx = Some(rx);

        Ok(())
    }

    /// Add a file to be watched in passive mode
    pub fn watch_file(&mut self, path: PathBuf, file_position: FilePosition) -> Result<(), notify::Error> {
        if self.watcher.is_none() {
            self.initialize(&path)?;
        }

        let state = NotifyWatcherState {
            path,
            file_position,
            last_seen: Instant::now(),
            is_active: false,
        };

        let mut states = self.states.lock().unwrap();
        states.push(state);

        Ok(())
    }

    /// Check for any events on the watched files
    ///
    /// Only returns events that indicate actual file changes (writes, moves, or renames)
    /// to avoid reacting to our own file accesses.
    pub fn check_events(&mut self) -> Vec<(PathBuf, EventKind)> {
        let mut events = Vec::new();

        if let Some(ref mut rx) = self.event_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        match event {
                            Ok(event) => {
                                trace!(message = "Received file event", ?event);

                                // Filter for relevant events only
                                let is_relevant = match event.kind {
                                    // File content was modified
                                    EventKind::Modify(notify::event::ModifyKind::Data(_)) => true,
                                    // File was created or moved
                                    EventKind::Create(_) => true,
                                    // File was renamed
                                    EventKind::Modify(notify::event::ModifyKind::Name(_)) => true,
                                    // Other events are not relevant for our purposes
                                    _ => false,
                                };

                                if is_relevant {
                                    for path in event.paths {
                                        trace!(message = "Relevant file event detected", ?path, kind = ?event.kind);
                                        events.push((path, event.kind));
                                    }
                                } else {
                                    trace!(message = "Ignoring non-relevant file event", kind = ?event.kind);
                                }
                            }
                            Err(e) => {
                                error!(message = "Error receiving file event", error = ?e);
                            }
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        // No more events to process
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        error!(message = "Notify watcher channel disconnected");
                        break;
                    }
                }
            }
        }

        events
    }

    /// Activate watching for a file
    #[allow(dead_code)]
    pub fn activate(&mut self, path: &Path) -> bool {
        let mut states = self.states.lock().unwrap();
        for state in states.iter_mut() {
            if state.path == path {
                state.is_active = true;
                state.last_seen = Instant::now();
                return true;
            }
        }
        false
    }

    /// Deactivate watching for a file (switch to passive mode)
    #[allow(dead_code)]
    pub fn deactivate(&mut self, path: &Path, file_position: FilePosition) -> bool {
        let mut states = self.states.lock().unwrap();
        for state in states.iter_mut() {
            if state.path == path {
                state.is_active = false;
                state.file_position = file_position;
                state.last_seen = Instant::now();
                return true;
            }
        }
        false
    }

    /// Get the current file position for a path
    #[allow(dead_code)]
    pub fn get_file_position(&self, path: &Path) -> Option<FilePosition> {
        let states = self.states.lock().unwrap();
        for state in states.iter() {
            if state.path == path {
                return Some(state.file_position);
            }
        }
        None
    }

    /// Update the file position for a path
    #[allow(dead_code)]
    pub fn update_file_position(&mut self, path: &Path, file_position: FilePosition) {
        let mut states = self.states.lock().unwrap();
        for state in states.iter_mut() {
            if state.path == path {
                state.file_position = file_position;
                state.last_seen = Instant::now();
                break;
            }
        }
    }
}
