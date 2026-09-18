use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use glob::{MatchOptions, Pattern};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tokio::sync::{mpsc, Notify};
use tracing::{debug, error, warn};

use super::PathsProvider;
use crate::FileSourceInternalEvents;

/// Discovers matching paths using notifications and periodic glob reconciliation.
pub struct NotifyPathsProvider<E: FileSourceInternalEvents> {
    include_patterns: Vec<Pattern>,
    exclude_patterns: Vec<Pattern>,
    glob_match_options: MatchOptions,
    discovered_files: HashSet<PathBuf>,
    watcher: Option<notify::RecommendedWatcher>,
    events: mpsc::Receiver<notify::Result<Event>>,
    needs_rescan: Arc<AtomicBool>,
    changed: Arc<Notify>,
    emitter: E,
}

impl<E: FileSourceInternalEvents> NotifyPathsProvider<E> {
    /// Register directory watches. The first `paths(true)` performs initial discovery.
    pub fn new(
        include_patterns: &[PathBuf],
        exclude_patterns: &[PathBuf],
        glob_match_options: MatchOptions,
        emitter: E,
    ) -> Self {
        let compile_patterns = |paths: &[PathBuf]| {
            paths
                .iter()
                .map(|path| Pattern::new(&path.to_string_lossy()).expect("Invalid glob pattern"))
                .collect()
        };
        let (send, events) = mpsc::channel(100);
        let needs_rescan = Arc::new(AtomicBool::new(false));
        let overflow = Arc::clone(&needs_rescan);
        let changed = Arc::new(Notify::new());
        let wake = Arc::clone(&changed);
        let watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if matches!(&event, Ok(event) if matches!(event.kind, EventKind::Access(_) | EventKind::Other))
            {
                return;
            }
            // Never block the notification thread: a full queue is repaired by a scan.
            if matches!(
                send.try_send(event),
                Err(mpsc::error::TrySendError::Full(_))
            ) {
                overflow.store(true, Ordering::Relaxed);
            }
            wake.notify_one();
        });
        let mut provider = Self {
            include_patterns: compile_patterns(include_patterns),
            exclude_patterns: compile_patterns(exclude_patterns),
            glob_match_options,
            discovered_files: HashSet::new(),
            watcher: None,
            events,
            needs_rescan,
            changed,
            emitter,
        };
        let registration = watcher.and_then(|watcher| {
            provider.watcher = Some(watcher);
            provider.watch_directories()
        });
        if let Err(error) = registration {
            warn!(
                ?error,
                "Failed to initialize notify watcher, falling back to glob scanning"
            );
        }
        provider
    }

    fn process_events(&mut self) -> bool {
        let mut rescan = self.needs_rescan.swap(false, Ordering::Relaxed);
        while let Ok(event) = self.events.try_recv() {
            let event = match event {
                Ok(event) => event,
                Err(error) => {
                    error!(?error, "Error receiving file discovery event");
                    rescan = true;
                    continue;
                }
            };
            // Directory moves/creation can introduce files before the OS watcher
            // registers them. Unknown or lost events also require reconciliation.
            rescan |= event.need_rescan()
                || matches!(
                    event.kind,
                    EventKind::Any
                        | EventKind::Create(
                            notify::event::CreateKind::Any | notify::event::CreateKind::Folder
                        )
                        | EventKind::Remove(
                            notify::event::RemoveKind::Any | notify::event::RemoveKind::Folder
                        )
                        | EventKind::Modify(notify::event::ModifyKind::Name(_))
                );
            match event.kind {
                EventKind::Remove(_) => {
                    for path in event.paths {
                        self.discovered_files.remove(&path);
                    }
                }
                EventKind::Create(_) | EventKind::Modify(_) => {
                    for path in event.paths {
                        if self.matches(&path) {
                            self.discovered_files.insert(path);
                        }
                    }
                }
                _ => {}
            }
        }
        rescan
    }

    fn matches(&self, path: &Path) -> bool {
        self.include_patterns
            .iter()
            .any(|pattern| pattern.matches_path_with(path, self.glob_match_options))
            && !self
                .exclude_patterns
                .iter()
                .any(|pattern| pattern.matches_path(path))
    }
    /// Watch the literal directory prefix so future glob matches are covered.
    fn watch_directories(&mut self) -> Result<(), notify::Error> {
        let mut roots = BTreeMap::new();
        for pattern in &self.include_patterns {
            let directory = Path::new(pattern.as_str())
                .parent()
                .unwrap_or_else(|| Path::new("."));
            let mut root = PathBuf::new();
            let mut recursive = false;
            for component in directory.components() {
                if component
                    .as_os_str()
                    .to_string_lossy()
                    .contains(['*', '?', '['])
                {
                    recursive = true;
                    break;
                }
                root.push(component.as_os_str());
            }
            if root.as_os_str().is_empty() {
                root.push(".");
            }
            // A second pattern must not downgrade an existing recursive watch.
            roots
                .entry(root)
                .and_modify(|value| *value |= recursive)
                .or_insert(recursive);
        }

        if let Some(watcher) = &mut self.watcher {
            for (root, recursive) in roots {
                let mode = if recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                // Propagate registration failures to enable glob fallback.
                watcher.watch(&root, mode)?;
                debug!(directory = ?root, ?mode, "Watching directory for file discovery");
            }
        }
        Ok(())
    }

    async fn glob_scan(&mut self) {
        let include_patterns = self.include_patterns.clone();
        let exclude_patterns = self.exclude_patterns.clone();
        let options = self.glob_match_options;
        let emitter = self.emitter.clone();
        let scan = tokio::task::spawn_blocking(move || {
            let mut files = HashSet::new();
            let mut complete = true;
            for pattern in include_patterns {
                for entry in
                    glob::glob_with(pattern.as_str(), options).expect("Invalid glob pattern")
                {
                    match entry {
                        Ok(path) => {
                            if !exclude_patterns
                                .iter()
                                .any(|pattern| pattern.matches_path(&path))
                            {
                                files.insert(path);
                            }
                        }
                        Err(error) => {
                            emitter.emit_path_globbing_failed(error.path(), error.error());
                            complete = false;
                        }
                    }
                }
            }
            (files, complete)
        })
        .await;
        match scan {
            Ok((files, true)) => self.discovered_files = files,
            // A failed scan cannot establish which cached paths have disappeared.
            Ok((files, false)) => self.discovered_files.extend(files),
            Err(error) => error!(?error, "Error during glob scan"),
        }
    }
}

impl<E: FileSourceInternalEvents> PathsProvider for NotifyPathsProvider<E> {
    type IntoIter = Vec<PathBuf>;

    fn wait_for_changes(&mut self) -> impl std::future::Future<Output = ()> + Send {
        self.changed.notified()
    }

    async fn paths(&mut self, should_glob: bool) -> Self::IntoIter {
        let needs_rescan = self.process_events();
        if should_glob || needs_rescan {
            self.glob_scan().await;
            // Apply notifications received during the scan after its snapshot.
            // Keep any requested reconciliation pending for the next call.
            if self.process_events() {
                self.needs_rescan.store(true, Ordering::Relaxed);
            }
        }
        self.discovered_files.iter().cloned().collect()
    }
}
