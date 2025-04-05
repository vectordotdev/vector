use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use glob::{MatchOptions, Pattern};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{debug, error, trace, warn};

use crate::FileSourceInternalEvents;
use super::PathsProvider;

/// Metadata about a discovered file
#[derive(Debug, Clone)]
struct FileMetadata {
    /// Last time the file was seen
    last_seen: Instant,
}

/// A notify-based path provider.
///
/// Uses filesystem notifications to discover files that match include patterns
/// and don't match the exclude patterns, instead of continuously globbing.
pub struct NotifyPathsProvider<E: FileSourceInternalEvents> {
    /// Patterns to include
    include_patterns: Vec<String>,
    /// Patterns to exclude
    exclude_patterns: Vec<Pattern>,
    /// Options for glob pattern matching
    glob_match_options: MatchOptions,
    /// Cache of discovered files
    discovered_files: Arc<DashMap<PathBuf, FileMetadata>>,
    /// The underlying notify watcher
    watcher: Option<RecommendedWatcher>,
    /// Receiver for notify events
    event_rx: Option<std::sync::mpsc::Receiver<Result<Event, notify::Error>>>,
    /// Last time we did a full glob scan (as fallback)
    last_glob_scan: Instant,
    /// Minimum time between full glob scans
    glob_minimum_cooldown: Duration,
    /// Event emitter
    emitter: E,
}

impl<E: FileSourceInternalEvents> NotifyPathsProvider<E> {
    /// Create a new NotifyPathsProvider
    pub fn new(
        include_patterns: &[PathBuf],
        exclude_patterns: &[PathBuf],
        glob_match_options: MatchOptions,
        glob_minimum_cooldown: Duration,
        emitter: E,
    ) -> Self {
        // Convert exclude patterns to Pattern objects
        let exclude_patterns = exclude_patterns
            .iter()
            .map(|path_buf| {
                let path_str = path_buf.to_string_lossy();
                Pattern::new(&path_str).expect("Invalid exclude pattern")
            })
            .collect::<Vec<Pattern>>();

        // Convert include patterns to strings
        let include_patterns = include_patterns
            .iter()
            .map(|path_buf| path_buf.to_string_lossy().to_string())
            .collect::<Vec<String>>();

        // Create the provider
        let mut provider = NotifyPathsProvider {
            include_patterns,
            exclude_patterns,
            glob_match_options,
            discovered_files: Arc::new(DashMap::new()),
            watcher: None,
            event_rx: None,
            last_glob_scan: Instant::now().checked_sub(glob_minimum_cooldown).unwrap_or_else(Instant::now),
            glob_minimum_cooldown,
            emitter,
        };

        // Initialize the watcher
        if let Err(e) = provider.initialize_watcher() {
            warn!(message = "Failed to initialize notify watcher, falling back to glob scanning", error = ?e);
        }

        // Do an initial glob scan to discover existing files
        provider.glob_scan();

        provider
    }

    /// Initialize the notify watcher
    fn initialize_watcher(&mut self) -> Result<(), notify::Error> {
        // Create a channel for receiving events
        let (tx, rx) = std::sync::mpsc::channel();

        // Create a custom config that's optimized for our use case
        let config = Config::default()
            // Use a reasonable polling interval as fallback
            .with_poll_interval(Duration::from_secs(2))
            // We only care about file modifications, creations, and renames
            .with_compare_contents(false);

        // Create the watcher with our custom config
        let mut watcher = RecommendedWatcher::new(tx, config)?;

        // Watch the directories that match our include patterns
        for pattern in &self.include_patterns {
            // Extract the directory part of the pattern
            let dir_pattern = if let Some(last_slash) = pattern.rfind('/') {
                &pattern[..last_slash]
            } else {
                "."
            };

            // If the directory pattern contains wildcards, we need to find all matching directories
            if dir_pattern.contains('*') || dir_pattern.contains('?') || dir_pattern.contains('[') {
                // Use glob to find all matching directories
                if let Ok(entries) = glob::glob(dir_pattern) {
                    for entry in entries.filter_map(Result::ok) {
                        if entry.is_dir() {
                            if let Err(e) = watcher.watch(&entry, RecursiveMode::Recursive) {
                                warn!(
                                    message = "Failed to watch directory",
                                    directory = ?entry,
                                    error = ?e
                                );
                            } else {
                                debug!(
                                    message = "Watching directory for file discovery",
                                    directory = ?entry
                                );
                            }
                        }
                    }
                }
            } else {
                // The directory pattern is a literal path
                let dir = PathBuf::from(dir_pattern);
                if dir.is_dir() {
                    if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
                        warn!(
                            message = "Failed to watch directory",
                            directory = ?dir,
                            error = ?e
                        );
                    } else {
                        debug!(
                            message = "Watching directory for file discovery",
                            directory = ?dir
                        );
                    }
                }
            }
        }

        self.watcher = Some(watcher);
        self.event_rx = Some(rx);

        Ok(())
    }

    /// Process any pending notify events
    fn process_events(&self) {
        if let Some(ref rx) = self.event_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        match event {
                            Ok(event) => {
                                trace!(message = "Received file discovery event", ?event);

                                // Filter for relevant events only
                                let is_relevant = match event.kind {
                                    // File was created
                                    EventKind::Create(notify::event::CreateKind::File) => true,
                                    // File was moved
                                    EventKind::Modify(notify::event::ModifyKind::Name(
                                        notify::event::RenameMode::To
                                    )) => true,
                                    // Other events are not relevant for discovery
                                    _ => false,
                                };

                                if is_relevant {
                                    for path in event.paths {
                                        // Check if the path matches our patterns
                                        if self.matches_patterns(&path) {
                                            trace!(
                                                message = "Discovered new file via notification",
                                                ?path
                                            );
                                            // Add the file to our cache
                                            self.discovered_files.insert(
                                                path,
                                                FileMetadata {
                                                    last_seen: Instant::now(),
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!(message = "Error receiving file discovery event", error = ?e);
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
    }

    /// Perform a full glob scan as fallback
    fn glob_scan(&mut self) {
        // Check if we need to do a full glob scan
        let now = Instant::now();
        if now.duration_since(self.last_glob_scan) < self.glob_minimum_cooldown {
            return;
        }

        trace!(message = "Performing full glob scan for file discovery");
        self.last_glob_scan = now;

        // Use the traditional glob approach to find files
        let files = self.include_patterns
            .iter()
            .flat_map(|include_pattern| {
                glob::glob_with(include_pattern.as_str(), self.glob_match_options)
                    .expect("failed to read glob pattern")
                    .filter_map(|val| {
                        val.map_err(|error| {
                            self.emitter
                                .emit_path_globbing_failed(error.path(), error.error())
                        })
                        .ok()
                    })
            })
            .filter(|candidate_path: &PathBuf| -> bool {
                !self.exclude_patterns.iter().any(|exclude_pattern| {
                    let candidate_path_str = candidate_path.to_str().unwrap();
                    exclude_pattern.matches(candidate_path_str)
                })
            })
            .collect::<Vec<PathBuf>>();

        // Update our cache with the discovered files
        for path in files {
            self.discovered_files.entry(path).or_insert_with(|| {
                FileMetadata {
                    last_seen: now,
                }
            });
        }

        // Update the last_seen timestamp for all files
        for mut entry in self.discovered_files.iter_mut() {
            entry.last_seen = now;
        }

        // Remove files that haven't been seen in a while
        self.discovered_files.retain(|_, metadata| {
            now.duration_since(metadata.last_seen) < Duration::from_secs(300) // 5 minutes
        });
    }

    /// Check if a path matches our include/exclude patterns
    fn matches_patterns(&self, path: &PathBuf) -> bool {
        // Check if the path matches any include pattern
        let path_str = path.to_str().unwrap_or_default();
        let matches_include = self.include_patterns.iter().any(|pattern| {
            glob::Pattern::new(pattern)
                .map(|p| p.matches(path_str))
                .unwrap_or(false)
        });

        // Check if the path matches any exclude pattern
        let matches_exclude = self.exclude_patterns.iter().any(|pattern| {
            pattern.matches(path_str)
        });

        matches_include && !matches_exclude
    }
}

impl<E: FileSourceInternalEvents> PathsProvider for NotifyPathsProvider<E> {
    type IntoIter = Vec<PathBuf>;

    fn paths(&self) -> Self::IntoIter {
        // Process any pending events
        self.process_events();

        // Return the current set of discovered files
        self.discovered_files.iter().map(|entry| entry.key().clone()).collect()
    }
}
