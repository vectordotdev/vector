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

use super::{PathUpdates, PathsProvider};
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

    fn process_events(&mut self) -> (bool, HashSet<PathBuf>) {
        let mut changed_paths = HashSet::new();
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
            changed_paths.extend(
                event
                    .paths
                    .iter()
                    .filter(|path| self.matches(path))
                    .cloned(),
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
        (rescan, changed_paths)
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
    fn wait_for_changes(&mut self) -> impl std::future::Future<Output = ()> + Send {
        self.changed.notified()
    }

    async fn paths(&mut self, should_glob: bool) -> PathUpdates {
        let (needs_rescan, changed_paths) = self.process_events();
        if should_glob || needs_rescan {
            self.glob_scan().await;
            // Apply notifications received during the scan after its snapshot.
            // Keep any requested reconciliation pending for the next call.
            if self.process_events().0 {
                self.needs_rescan.store(true, Ordering::Relaxed);
            }
            PathUpdates::Snapshot(self.discovered_files.clone())
        } else {
            let (updated, removed) = changed_paths
                .into_iter()
                .partition(|path| self.discovered_files.contains(path));
            PathUpdates::Changed { updated, removed }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use std::{io::Error, time::Duration};

    #[derive(Clone)]
    struct NoopEvents;

    impl FileSourceInternalEvents for NoopEvents {
        fn emit_file_added(&self, _: &Path) {}
        fn emit_file_resumed(&self, _: &Path, _: u64) {}
        fn emit_file_watch_error(&self, _: &Path, _: Error) {}
        fn emit_file_unwatched(&self, _: &Path, _: bool) {}
        fn emit_file_deleted(&self, _: &Path) {}
        fn emit_file_delete_error(&self, _: &Path, _: Error) {}
        fn emit_file_fingerprint_read_error(&self, _: &Path, _: Error) {}
        fn emit_file_checkpointed(&self, _: usize, _: Duration) {}
        fn emit_file_checksum_failed(&self, _: &Path) {}
        fn emit_file_checkpoint_write_error(&self, _: Error) {}
        fn emit_files_open(&self, _: usize) {}
        fn emit_path_globbing_failed(&self, _: &Path, _: &Error) {}
        fn emit_file_line_too_long(&self, _: &BytesMut, _: usize, _: usize) {}
    }

    // Drive notifications explicitly so tests do not depend on OS coalescing or timing.
    fn provider(
        root: &Path,
    ) -> (
        NotifyPathsProvider<NoopEvents>,
        mpsc::Sender<notify::Result<Event>>,
    ) {
        let (send, events) = mpsc::channel(100);
        (
            NotifyPathsProvider {
                include_patterns: vec![Pattern::new(root.join("*.log").to_str().unwrap()).unwrap()],
                exclude_patterns: Vec::new(),
                glob_match_options: MatchOptions::default(),
                discovered_files: HashSet::new(),
                watcher: None,
                events,
                needs_rescan: Arc::new(AtomicBool::new(false)),
                changed: Arc::new(Notify::new()),
                emitter: NoopEvents,
            },
            send,
        )
    }

    #[tokio::test]
    async fn notifications_only_return_affected_paths_once() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first.log");
        let second = root.path().join("second.log");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        let (mut provider, send) = provider(root.path());
        assert_eq!(
            provider.paths(true).await,
            PathUpdates::Snapshot([first.clone(), second.clone()].into())
        );
        assert_eq!(
            provider.paths(false).await,
            PathUpdates::Changed {
                updated: HashSet::new(),
                removed: HashSet::new()
            }
        );
        for _ in 0..3 {
            send.send(Ok(Event::new(EventKind::Modify(
                notify::event::ModifyKind::Any,
            ))
            .add_path(first.clone())))
                .await
                .unwrap();
        }
        send.send(Ok(Event::new(EventKind::Create(
            notify::event::CreateKind::File,
        ))
        .add_path(root.path().join("excluded.txt"))))
            .await
            .unwrap();
        assert_eq!(
            provider.paths(false).await,
            PathUpdates::Changed {
                updated: [first.clone()].into(),
                removed: HashSet::new()
            }
        );
        assert_eq!(
            provider.paths(false).await,
            PathUpdates::Changed {
                updated: HashSet::new(),
                removed: HashSet::new()
            }
        );
        std::fs::remove_file(&first).unwrap();
        send.send(Ok(Event::new(EventKind::Remove(
            notify::event::RemoveKind::File,
        ))
        .add_path(first.clone())))
            .await
            .unwrap();
        assert_eq!(
            provider.paths(false).await,
            PathUpdates::Changed {
                updated: HashSet::new(),
                removed: [first].into()
            }
        );
        assert_eq!(
            provider.paths(true).await,
            PathUpdates::Snapshot([second].into())
        );
    }

    #[tokio::test]
    async fn reconciliation_recovers_missed_and_overflowed_notifications() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first.log");
        let second = root.path().join("second.log");
        std::fs::write(&first, "first").unwrap();
        let (mut provider, _send) = provider(root.path());
        assert_eq!(
            provider.paths(true).await,
            PathUpdates::Snapshot([first.clone()].into())
        );
        std::fs::remove_file(&first).unwrap();
        std::fs::write(&second, "second").unwrap();
        // No event arrived: incremental reads do no discovery work.
        assert_eq!(
            provider.paths(false).await,
            PathUpdates::Changed {
                updated: HashSet::new(),
                removed: HashSet::new()
            }
        );
        assert_eq!(
            provider.paths(true).await,
            PathUpdates::Snapshot([second.clone()].into())
        );
        std::fs::write(&first, "first again").unwrap();
        // The callback sets this flag when its bounded queue overflows.
        provider.needs_rescan.store(true, Ordering::Relaxed);
        assert_eq!(
            provider.paths(false).await,
            PathUpdates::Snapshot([first, second].into())
        );
        assert_eq!(
            provider.paths(false).await,
            PathUpdates::Changed {
                updated: HashSet::new(),
                removed: HashSet::new()
            }
        );
    }
}
