use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use std::time::{Duration, Instant};

use dashmap::DashMap;
use glob::{MatchOptions, Pattern};
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::Mutex;
use tracing::{debug, error, trace, warn};

use super::PathsProvider;
use crate::FileSourceInternalEvents;

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
pub struct NotifyPathsProvider<E: FileSourceInternalEvents + Clone> {
    /// Patterns to include
    include_patterns: Vec<String>,
    /// Patterns to exclude
    exclude_patterns: Vec<Pattern>,
    /// Options for glob pattern matching
    glob_match_options: MatchOptions,
    /// Cache of discovered files
    discovered_files: Arc<DashMap<PathBuf, FileMetadata>>,
    /// The underlying notify watcher
    watcher: Option<notify::RecommendedWatcher>,

    /// Event emitter
    emitter: E,
    /// Mutex for thread-safe access to the event receiver
    event_mutex: Arc<Mutex<()>>,
    /// Flag to indicate if we should use glob scanning as fallback
    use_glob_fallback: bool,
}

impl<E: FileSourceInternalEvents> NotifyPathsProvider<E> {
    /// Create a new NotifyPathsProvider
    pub fn new(
        include_patterns: &[PathBuf],
        exclude_patterns: &[PathBuf],
        glob_match_options: MatchOptions,
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

            emitter,
            event_mutex: Arc::new(Mutex::new(())),
            use_glob_fallback: false, // Default to not using glob fallback
        };

        // Initialize the watcher
        if let Err(e) = provider.initialize_watcher() {
            warn!(message = "Failed to initialize notify watcher, falling back to glob scanning", error = ?e);
            provider.use_glob_fallback = true;
        }

        // Do an initial glob scan to discover existing files
        // We'll do this synchronously to avoid blocking issues
        // This is a simplified version that doesn't use async/await
        let now = Instant::now();

        // Use the traditional glob approach to find files
        let files = provider
            .include_patterns
            .iter()
            .flat_map(|include_pattern| {
                glob::glob_with(include_pattern.as_str(), provider.glob_match_options)
                    .expect("failed to read glob pattern")
                    .filter_map(|val| {
                        val.map_err(|error| {
                            provider
                                .emitter
                                .emit_path_globbing_failed(error.path(), error.error())
                        })
                        .ok()
                    })
            })
            .filter(|candidate_path: &PathBuf| -> bool {
                !provider.exclude_patterns.iter().any(|exclude_pattern| {
                    let candidate_path_str = candidate_path.to_str().unwrap();
                    exclude_pattern.matches(candidate_path_str)
                })
            })
            .collect::<Vec<PathBuf>>();

        // Update our cache with the discovered files
        for path in files {
            debug!(message = "Discovered file via initial glob scan", ?path);
            provider
                .discovered_files
                .entry(path)
                .or_insert_with(|| FileMetadata { last_seen: now });
        }

        provider
    }

    /// Initialize the notify watcher
    fn initialize_watcher(&mut self) -> Result<(), notify::Error> {
        // We don't need a custom config anymore since we're using recommended_watcher

        // Create a clone of the discovered_files for the callback
        let discovered_files = self.discovered_files.clone();
        let include_patterns = self.include_patterns.clone();
        let exclude_patterns = self.exclude_patterns.clone();

        // Create the watcher with a callback
        let watcher = notify::recommended_watcher(
            move |res: Result<notify::Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // Don't log Access events or Other events at all to eliminate noise
                        match event.kind {
                            EventKind::Access(_) => {
                                // Skip logging for Access events
                            }
                            EventKind::Other => {
                                // Skip logging for all Other events
                            }
                            _ => {
                                trace!(message = "Received file discovery event", ?event);
                            }
                        }

                        // Filter for relevant events only
                        let is_relevant = match event.kind {
                            // File was created
                            EventKind::Create(notify::event::CreateKind::File) => true,
                            // File was modified
                            EventKind::Modify(notify::event::ModifyKind::Data(_)) => true,
                            // File was moved
                            EventKind::Modify(notify::event::ModifyKind::Name(
                                notify::event::RenameMode::To,
                            )) => true,
                            // File was removed
                            EventKind::Remove(notify::event::RemoveKind::File) => true,
                            // Explicitly filter out Access events
                            EventKind::Access(_) => false,
                            // Explicitly filter out all Other events
                            EventKind::Other => false,
                            // Other events are not relevant for discovery
                            _ => false,
                        };

                        if is_relevant {
                            let now = Instant::now();
                            for path in event.paths {
                                // Convert to PathBuf
                                let path_buf = path.to_path_buf();

                                // Check if this is a removal event
                                if let EventKind::Remove(notify::event::RemoveKind::File) =
                                    event.kind
                                {
                                    // If the file was removed, remove it from our cache
                                    if discovered_files.contains_key(&path_buf) {
                                        debug!(
                                            message = "Removing deleted file from discovered files cache",
                                            path = ?path_buf
                                        );
                                        discovered_files.remove(&path_buf);
                                    }
                                    continue;
                                }

                                // Check if the path matches our patterns
                                let path_str = path_buf.to_str().unwrap_or_default();
                                let matches_include = include_patterns.iter().any(|pattern| {
                                    glob::Pattern::new(pattern)
                                        .map(|p| p.matches(path_str))
                                        .unwrap_or(false)
                                });

                                let matches_exclude = exclude_patterns
                                    .iter()
                                    .any(|pattern| pattern.matches(path_str));

                                if matches_include && !matches_exclude {
                                    debug!(
                                        message = "Discovered new file via notification",
                                        ?path_buf
                                    );
                                    // Add the file to our cache
                                    discovered_files
                                        .insert(path_buf.clone(), FileMetadata { last_seen: now });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(message = "Error receiving file discovery event", error = ?e);
                    }
                }
            },
        )?;

        // Store the watcher
        self.watcher = Some(watcher);

        // Watch directories
        self.watch_directories()?;

        Ok(())
    }

    /// Watch directories that match our include patterns
    fn watch_directories(&mut self) -> Result<(), notify::Error> {
        if let Some(ref mut watcher) = self.watcher {
            // Watch the directories that match our include patterns
            for pattern in &self.include_patterns {
                // Extract the directory part of the pattern
                let dir_pattern = if let Some(last_slash) = pattern.rfind('/') {
                    &pattern[..last_slash]
                } else {
                    "."
                };

                // If the directory pattern contains wildcards, we need to find all matching directories
                if dir_pattern.contains('*')
                    || dir_pattern.contains('?')
                    || dir_pattern.contains('[')
                {
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
        }

        Ok(())
    }

    /// Process any pending notify events
    async fn process_events(&mut self) {
        // Nothing to do here - events are processed by the callback
        // that was registered when the watcher was created
    }

    /// Perform a full glob scan as fallback
    async fn glob_scan(&mut self) {
        let now = Instant::now();
        debug!(message = "Performing full glob scan for file discovery");

        // Use tokio to run the glob scan in a blocking task to avoid blocking the async runtime
        let include_patterns = self.include_patterns.clone();
        let exclude_patterns = self.exclude_patterns.clone();
        let glob_match_options = self.glob_match_options;
        let emitter = self.emitter.clone();

        let files = tokio::task::spawn_blocking(move || {
            // Use the traditional glob approach to find files
            include_patterns
                .iter()
                .flat_map(|include_pattern| {
                    glob::glob_with(include_pattern.as_str(), glob_match_options)
                        .expect("failed to read glob pattern")
                        .filter_map(|val| {
                            val.map_err(|error| {
                                emitter.emit_path_globbing_failed(error.path(), error.error())
                            })
                            .ok()
                        })
                })
                .filter(|candidate_path: &PathBuf| -> bool {
                    !exclude_patterns.iter().any(|exclude_pattern| {
                        let candidate_path_str = candidate_path.to_str().unwrap();
                        exclude_pattern.matches(candidate_path_str)
                    })
                })
                .collect::<Vec<PathBuf>>()
        })
        .await
        .unwrap_or_else(|e| {
            error!(message = "Error during glob scan", error = ?e);
            Vec::new()
        });

        // Update our cache with the discovered files
        for path in files {
            debug!(message = "Discovered file via glob scan", ?path);
            self.discovered_files
                .entry(path)
                .or_insert_with(|| FileMetadata { last_seen: now });
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

    // The matches_patterns methods have been removed since they're not used
}

impl<E: FileSourceInternalEvents + Clone> Clone for NotifyPathsProvider<E> {
    fn clone(&self) -> Self {
        // Create a new instance with the same configuration
        // but without the watcher and event_rx
        NotifyPathsProvider {
            include_patterns: self.include_patterns.clone(),
            exclude_patterns: self.exclude_patterns.clone(),
            glob_match_options: self.glob_match_options,
            discovered_files: self.discovered_files.clone(),
            watcher: None, // Don't clone the watcher

            emitter: self.emitter.clone(),
            event_mutex: self.event_mutex.clone(),
            use_glob_fallback: self.use_glob_fallback,
        }
    }
}

impl<E: FileSourceInternalEvents + Clone + Send + Sync + 'static> PathsProvider
    for NotifyPathsProvider<E>
{
    type IntoIter = Vec<PathBuf>;

    fn paths(
        &self,
        should_glob: bool,
    ) -> Pin<Box<dyn Future<Output = Self::IntoIter> + Send + '_>> {
        // Clone everything we need to avoid capturing self
        let discovered_files = self.discovered_files.clone();
        let event_mutex = self.event_mutex.clone();
        let mut clone = self.clone();
        let use_glob_fallback = self.use_glob_fallback;

        Box::pin(async move {
            // Lock the mutex to ensure only one thread is processing events at a time
            let _lock = event_mutex.lock().await;

            // Process any pending events from the notify watcher
            // This will update the discovered_files with any new files
            clone.process_events().await;

            // Only perform a glob scan if we're using glob fallback
            // This prevents unnecessary glob scans when the notify watcher is working
            if use_glob_fallback && should_glob {
                debug!(message = "Using glob fallback for file discovery");
                clone.glob_scan().await;
            }

            // Return the current set of discovered files
            discovered_files
                .iter()
                .map(|entry| entry.key().clone())
                .collect()
        })
    }
}
