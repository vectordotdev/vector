use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError, TrySendError},
    },
};

use file_source_common::FileFingerprint;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::sync::Notify;
use tracing::{debug, warn};

use crate::scheduler::{FsEvent, FsEventSource};

const EVENT_CHANNEL_CAPACITY: usize = 4096;

#[derive(Debug)]
struct WatchedFileRecord {
    watched_path: PathBuf,
    watched_parent: Option<PathBuf>,
    canonical_target: Option<PathBuf>,
    target_parent: Option<PathBuf>,
}

/// Production [`FsEventSource`] backed by `notify::RecommendedWatcher`
/// (`inotify` on Linux, `FSEvents` on macOS, `ReadDirectoryChangesW` on Windows).
///
/// Events are treated strictly as **hints**; [`crate::scheduler::AdaptiveEventScheduler`]
/// always retains bounded fallback polling when notifications are delayed, dropped, or
/// unavailable (e.g. NFS/SMB or watch-exhausted hosts).
///
/// Design properties:
/// - **Per-watcher `FileFingerprint` tracking:** Avoids symlink-rotation races where a
///   new file at the same path is watched before the old watcher is reaped.
/// - **Parent-directory ref-counted watches:** Watches unique parent directories
///   (`RecursiveMode::NonRecursive`) plus canonical symlink target directories, keeping
///   OS watch descriptors `O(directories)` instead of `O(files)`.
/// - **Noisy-sibling filtering:** Checks `Modify`/`Remove` events against the active
///   tracked path set in the watcher callback so writes to unrelated files in shared
///   directories (e.g. `/var/log`) do not fill the channel or wake `FileServer`.
/// - **Bounded channel with overflow coalescing:** Channel saturation or watcher errors
///   coalesce into a single [`FsEvent::Overflow`].
pub struct NotifyEventSource {
    watcher: RecommendedWatcher,
    rx: Receiver<FsEvent>,
    overflowed: Arc<AtomicBool>,
    wakeup: Arc<Notify>,
    /// Shared set of tracked file paths (watched paths, canonical target paths, and
    /// canonicalized-parent forms) used to filter out noisy sibling files in the callback.
    tracked_paths: Arc<RwLock<HashSet<PathBuf>>>,
    /// Ref-count of each path in `tracked_paths` (handles multiple symlinks to same target).
    tracked_path_refs: HashMap<PathBuf, usize>,
    /// Active per-watcher registrations keyed by `FileFingerprint`.
    watched_files: HashMap<FileFingerprint, WatchedFileRecord>,
    /// Ref-count of watched files per canonical parent directory.
    dir_refs: HashMap<PathBuf, usize>,
    /// Maps canonicalized parent directory -> ref-counted original watched parent
    /// directory aliases when they differ (e.g. macOS `/var` -> `/private/var`).
    canonical_to_watched: HashMap<PathBuf, HashMap<PathBuf, usize>>,
    /// Maps canonicalized target file path -> ref-counted watched symlink path(s).
    file_aliases: HashMap<PathBuf, HashMap<PathBuf, usize>>,
}

impl NotifyEventSource {
    /// Creates a new `NotifyEventSource` using the platform's recommended OS watcher.
    ///
    /// # Errors
    /// Returns a `notify::Error` if initializing the OS file watcher fails
    /// (e.g. `inotify` instance limit reached).
    pub fn new() -> notify::Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<FsEvent>(EVENT_CHANNEL_CAPACITY);
        let overflowed = Arc::new(AtomicBool::new(false));
        let wakeup = Arc::new(Notify::new());
        let tracked_paths: Arc<RwLock<HashSet<PathBuf>>> = Arc::new(RwLock::new(HashSet::new()));

        let cb_overflowed = Arc::clone(&overflowed);
        let cb_wakeup = Arc::clone(&wakeup);
        let cb_tracked = Arc::clone(&tracked_paths);

        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let mut woke = false;
            match res {
                Ok(event) => {
                    if event.need_rescan() {
                        cb_overflowed.store(true, Ordering::Release);
                        woke = true;
                    } else if let Some(kind) = map_event_kind(event.kind) {
                        for path in event.paths {
                            // Filter out noisy sibling files for all event kinds;
                            // `AdaptiveEventScheduler` only wakes tracked paths.
                            let is_tracked =
                                cb_tracked.read().map_or(true, |set| set.contains(&path));
                            if !is_tracked {
                                continue;
                            }
                            let fs_event = match kind {
                                MappedKind::Modified => FsEvent::Modified(path),
                                MappedKind::Created => FsEvent::Created(path),
                                MappedKind::Removed => FsEvent::Removed(path),
                            };
                            match tx.try_send(fs_event) {
                                Ok(()) => woke = true,
                                Err(TrySendError::Full(_)) => {
                                    cb_overflowed.store(true, Ordering::Release);
                                    woke = true;
                                    break;
                                }
                                Err(TrySendError::Disconnected(_)) => return,
                            }
                        }
                    }
                }
                Err(error) => {
                    warn!(message = "OS file watcher error; falling back to rescan.", %error);
                    cb_overflowed.store(true, Ordering::Release);
                    woke = true;
                }
            }
            if woke {
                cb_wakeup.notify_one();
            }
        })?;

        Ok(Self {
            watcher,
            rx,
            overflowed,
            wakeup,
            tracked_paths,
            tracked_path_refs: HashMap::new(),
            watched_files: HashMap::new(),
            dir_refs: HashMap::new(),
            canonical_to_watched: HashMap::new(),
            file_aliases: HashMap::new(),
        })
    }

    /// Returns the number of currently watched unique parent directories.
    #[must_use]
    pub fn watched_dir_count(&self) -> usize {
        self.dir_refs.len()
    }

    fn track_path_ref(&mut self, path: PathBuf) {
        let count = self.tracked_path_refs.entry(path.clone()).or_insert(0);
        *count += 1;
        if *count == 1
            && let Ok(mut set) = self.tracked_paths.write()
        {
            set.insert(path);
        }
    }

    fn untrack_path_ref(&mut self, path: &Path) {
        if let Some(count) = self.tracked_path_refs.get_mut(path) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.tracked_path_refs.remove(path);
                if let Ok(mut set) = self.tracked_paths.write() {
                    set.remove(path);
                }
            }
        }
    }

    fn watch_dir(&mut self, dir: &Path) {
        let dir_buf = dir.to_path_buf();
        let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir_buf.clone());
        let count = self.dir_refs.entry(canonical.clone()).or_insert(0);
        *count += 1;
        if *count == 1
            && let Err(error) = self.watcher.watch(&canonical, RecursiveMode::NonRecursive)
        {
            debug!(
                message = "Failed to register OS directory watch; relying on fallback polling.",
                dir = ?canonical,
                %error,
            );
        }
        if canonical != dir_buf {
            *self
                .canonical_to_watched
                .entry(canonical)
                .or_default()
                .entry(dir_buf)
                .or_insert(0) += 1;
        }
    }

    fn unwatch_dir(&mut self, dir: &Path) {
        let canonical = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if canonical != dir
            && let Some(aliases) = self.canonical_to_watched.get_mut(&canonical)
        {
            if let Some(c) = aliases.get_mut(dir) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    aliases.remove(dir);
                }
            }
            if aliases.is_empty() {
                self.canonical_to_watched.remove(&canonical);
            }
        }
        if let Some(count) = self.dir_refs.get_mut(&canonical) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.dir_refs.remove(&canonical);
                if let Err(error) = self.watcher.unwatch(&canonical) {
                    debug!(
                        message = "Failed to unregister OS directory watch.",
                        dir = ?canonical,
                        %error,
                    );
                }
            }
        }
    }

    fn remap_event(event: &FsEvent, new_path: PathBuf) -> FsEvent {
        match event {
            FsEvent::Modified(_) => FsEvent::Modified(new_path),
            FsEvent::Created(_) => FsEvent::Created(new_path),
            FsEvent::Removed(_) => FsEvent::Removed(new_path),
            FsEvent::Overflow => FsEvent::Overflow,
        }
    }

    fn normalize_event(&self, event: FsEvent, out: &mut Vec<FsEvent>) {
        let path = match &event {
            FsEvent::Modified(p) | FsEvent::Created(p) | FsEvent::Removed(p) => p,
            FsEvent::Overflow => {
                out.push(event);
                return;
            }
        };

        // 1. Direct canonical target -> symlink path aliases (e.g. Kubernetes pod logs).
        let mut emitted_alias = false;
        if let Some(symlink_paths) = self.file_aliases.get(path) {
            for sym_path in symlink_paths.keys() {
                out.push(Self::remap_event(&event, sym_path.clone()));
                emitted_alias = true;
            }
        }

        // 2. Canonical parent dir -> watched parent dir aliases (e.g. macOS /private/var).
        if let (Some(parent), Some(file_name)) = (path.parent(), path.file_name())
            && let Some(aliases) = self.canonical_to_watched.get(parent)
        {
            for alias_parent in aliases.keys() {
                out.push(Self::remap_event(&event, alias_parent.join(file_name)));
            }
            if self.dir_refs.contains_key(parent) {
                out.push(event);
            }
            return;
        }

        if !emitted_alias || path.parent().is_some_and(|p| self.dir_refs.contains_key(p)) {
            out.push(event);
        }
    }
}

#[derive(Clone, Copy)]
enum MappedKind {
    Modified,
    Created,
    Removed,
}

const fn map_event_kind(kind: EventKind) -> Option<MappedKind> {
    match kind {
        EventKind::Create(_) => Some(MappedKind::Created),
        EventKind::Remove(_) => Some(MappedKind::Removed),
        EventKind::Modify(_) | EventKind::Any | EventKind::Other => Some(MappedKind::Modified),
        EventKind::Access(_) => None,
    }
}

impl FsEventSource for NotifyEventSource {
    fn drain_events(&mut self, out: &mut Vec<FsEvent>) {
        if self.overflowed.swap(false, Ordering::AcqRel) {
            // Bounded drain of stale buffered events; a single Overflow promotes all files.
            for _ in 0..EVENT_CHANNEL_CAPACITY {
                if self.rx.try_recv().is_err() {
                    break;
                }
            }
            debug!(
                message =
                    "OS file event queue overflowed; scheduling full rescan of watched files."
            );
            out.push(FsEvent::Overflow);
            return;
        }

        for _ in 0..EVENT_CHANNEL_CAPACITY {
            match self.rx.try_recv() {
                Ok(event) => self.normalize_event(event, out),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    out.push(FsEvent::Overflow);
                    return;
                }
            }
        }
        // Hit per-tick cap while events still pending -> coalesce into Overflow.
        self.overflowed.store(true, Ordering::Release);
        out.push(FsEvent::Overflow);
    }

    fn watch_path(&mut self, file_id: FileFingerprint, path: &Path) {
        // If `file_id` was already registered (e.g. rename), clean up its old record first.
        if self.watched_files.contains_key(&file_id) {
            self.unwatch_path(file_id);
        }

        let watched_path = path.to_path_buf();
        let watched_parent = path.parent().map(Path::to_path_buf);
        if let Some(parent) = &watched_parent {
            self.watch_dir(parent);
        }
        self.track_path_ref(watched_path.clone());

        // If `path` resolves to a different canonical path (symlink or symlinked parent dir),
        // also track & watch the canonical target's parent directory.
        let (canonical_target, target_parent) = if let Ok(canonical_file) =
            std::fs::canonicalize(path)
            && canonical_file != path
        {
            let tp = canonical_file
                .parent()
                .filter(|tp| Some(*tp) != path.parent())
                .map(Path::to_path_buf);
            if let Some(dir) = &tp {
                self.watch_dir(dir);
            }
            self.track_path_ref(canonical_file.clone());
            *self
                .file_aliases
                .entry(canonical_file.clone())
                .or_default()
                .entry(watched_path.clone())
                .or_insert(0) += 1;
            (Some(canonical_file), tp)
        } else {
            (None, None)
        };

        self.watched_files.insert(
            file_id,
            WatchedFileRecord {
                watched_path,
                watched_parent,
                canonical_target,
                target_parent,
            },
        );
    }

    fn unwatch_path(&mut self, file_id: FileFingerprint) {
        let Some(rec) = self.watched_files.remove(&file_id) else {
            return;
        };

        self.untrack_path_ref(&rec.watched_path);
        if let Some(parent) = &rec.watched_parent {
            self.unwatch_dir(parent);
        }
        if let Some(canonical_file) = &rec.canonical_target {
            self.untrack_path_ref(canonical_file);
            if let Some(aliases) = self.file_aliases.get_mut(canonical_file) {
                if let Some(c) = aliases.get_mut(&rec.watched_path) {
                    *c = c.saturating_sub(1);
                    if *c == 0 {
                        aliases.remove(&rec.watched_path);
                    }
                }
                if aliases.is_empty() {
                    self.file_aliases.remove(canonical_file);
                }
            }
        }
        if let Some(tp) = &rec.target_parent {
            self.unwatch_dir(tp);
        }
    }

    fn wakeup_notifier(&self) -> Option<Arc<Notify>> {
        Some(Arc::clone(&self.wakeup))
    }

    fn watched_file_count(&self) -> usize {
        self.watched_files.len()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Write as _,
        time::{Duration, Instant},
    };

    use bytes::Bytes;
    use file_source_common::{
        Checkpointer, FileFingerprint, FingerprintStrategy, Fingerprinter, OsFileSystem, ReadFrom,
        SystemClock,
    };
    use tempfile::tempdir;

    use crate::{
        file_server::{FileServer, FileServerState, Line},
        paths_provider::{Glob, MatchOptions},
        scheduler::{AdaptiveEventScheduler, AdaptiveSchedulerConfig, FsEvent, FsEventSource},
        sim::emitter::RecordingEmitter,
    };

    use super::NotifyEventSource;

    #[test]
    fn notify_event_source_watches_parent_dir_and_filters_noisy_siblings() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.log");
        let file_b = dir.path().join("b.log");
        let noisy_sibling = dir.path().join("unrelated.tmp");
        fs::write(&file_a, b"init_a\n").unwrap();
        fs::write(&file_b, b"init_b\n").unwrap();
        fs::write(&noisy_sibling, b"noise\n").unwrap();

        let fp_a = FileFingerprint::DevInode(1, 1);
        let fp_b = FileFingerprint::DevInode(1, 2);

        let mut source = NotifyEventSource::new().expect("create notify watcher");
        source.watch_path(fp_a, &file_a);
        source.watch_path(fp_b, &file_b);
        // Two files in the same directory -> only 1 OS directory watch.
        assert_eq!(source.watched_dir_count(), 1);
        assert_eq!(source.watched_file_count(), 2);

        // Drain any startup events, then write to the unrelated sibling file.
        let mut events = Vec::new();
        source.drain_events(&mut events);
        events.clear();

        let mut noisy = fs::OpenOptions::new()
            .append(true)
            .open(&noisy_sibling)
            .unwrap();
        writeln!(noisy, "should_be_ignored").unwrap();
        noisy.flush().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        source.drain_events(&mut events);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, FsEvent::Modified(p) if p == &noisy_sibling)),
            "unrelated sibling file should be filtered out, got {events:?}"
        );

        // Append to `file_a` and verify Modified event arrives.
        let mut f = fs::OpenOptions::new().append(true).open(&file_a).unwrap();
        writeln!(f, "hello_notify").unwrap();
        f.flush().unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            source.drain_events(&mut events);
            if events
                .iter()
                .any(|e| matches!(e, FsEvent::Modified(p) if p == &file_a))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(
            events
                .iter()
                .any(|e| matches!(e, FsEvent::Modified(p) if p == &file_a)),
            "expected Modified event for {file_a:?}, got {events:?}"
        );

        // Ref-counting: unwatching one file keeps directory watch; unwatching both removes it.
        source.unwatch_path(fp_a);
        assert_eq!(source.watched_dir_count(), 1);
        source.unwatch_path(fp_b);
        assert_eq!(source.watched_dir_count(), 0);
        assert_eq!(source.watched_file_count(), 0);
    }

    fn make_os_server(
        pattern: &std::path::Path,
        data_dir: &std::path::Path,
    ) -> FileServer<Glob<RecordingEmitter>, RecordingEmitter> {
        let emitter = RecordingEmitter::new();
        let paths_provider = Glob::new(
            std::slice::from_ref(&pattern.to_path_buf()),
            &[],
            MatchOptions::default(),
            emitter.clone(),
        )
        .unwrap();
        FileServer {
            paths_provider,
            max_read_bytes: 4096,
            ignore_checkpoints: false,
            read_from: ReadFrom::Beginning,
            ignore_before: None,
            max_line_bytes: 1024,
            line_delimiter: Bytes::from_static(b"\n"),
            data_dir: data_dir.to_path_buf(),
            glob_minimum_cooldown: Duration::from_secs(1),
            fingerprinter: Fingerprinter::new(FingerprintStrategy::DevInode, 1024, false),
            oldest_first: false,
            remove_after: None,
            emitter,
            rotate_wait: Duration::from_secs(60),
        }
    }

    /// Real-OS differential test: runs `AdaptiveEventScheduler + NotifyEventSource`
    /// vs `LegacyPollingScheduler` against real `TempDir` files on the host OS
    /// (300 idle files + 2 hot writers) and verifies identical line delivery and
    /// immediate `notify` wakeup.
    #[tokio::test]
    async fn real_os_adaptive_notify_vs_legacy_on_idle_heavy_dir() {
        let dir = tempdir().unwrap();
        let data_dir_legacy = tempdir().unwrap();
        let data_dir_adaptive = tempdir().unwrap();

        let idle_count = 300;
        for i in 0..idle_count {
            fs::write(dir.path().join(format!("idle_{i:03}.log")), b"seed\n").unwrap();
        }
        let hot0 = dir.path().join("hot_0.log");
        let hot1 = dir.path().join("hot_1.log");
        fs::write(&hot0, b"h0_seed\n").unwrap();
        fs::write(&hot1, b"h1_seed\n").unwrap();

        let pattern = dir.path().join("*.log");

        let mut cp_legacy = Checkpointer::new(data_dir_legacy.path());
        let mut legacy = FileServerState::init(
            make_os_server(&pattern, data_dir_legacy.path()),
            &mut cp_legacy,
            OsFileSystem,
            SystemClock,
        )
        .await;

        let mut cp_adaptive = Checkpointer::new(data_dir_adaptive.path());
        let notify_source = NotifyEventSource::new().expect("create notify watcher");
        let wakeup = notify_source.wakeup_notifier().unwrap();
        let mut adaptive = FileServerState::init_with_scheduler(
            make_os_server(&pattern, data_dir_adaptive.path()),
            &mut cp_adaptive,
            OsFileSystem,
            SystemClock,
            AdaptiveEventScheduler::new(AdaptiveSchedulerConfig::default()),
            notify_source,
        )
        .await;

        let mut legacy_lines: Vec<Line> = Vec::new();
        let mut adaptive_lines: Vec<Line> = Vec::new();
        let _ = legacy.step(&mut legacy_lines).await;
        let _ = adaptive.step(&mut adaptive_lines).await;
        assert_eq!(legacy_lines.len(), idle_count + 2);
        assert_eq!(adaptive_lines.len(), idle_count + 2);
        legacy_lines.clear();
        adaptive_lines.clear();

        let mut f0 = fs::OpenOptions::new().append(true).open(&hot0).unwrap();
        let mut f1 = fs::OpenOptions::new().append(true).open(&hot1).unwrap();
        let mut expected_hot = Vec::new();
        for i in 0..20 {
            let l0 = format!("hot0_msg_{i}");
            let l1 = format!("hot1_msg_{i}");
            writeln!(f0, "{l0}").unwrap();
            writeln!(f1, "{l1}").unwrap();
            expected_hot.push(l0);
            expected_hot.push(l1);
        }
        f0.flush().unwrap();
        f1.flush().unwrap();

        assert!(
            tokio::time::timeout(Duration::from_secs(5), wakeup.notified())
                .await
                .is_ok(),
            "expected NotifyEventSource to wake within 5s"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while adaptive_lines.len() < expected_hot.len() && Instant::now() < deadline {
            let _ = adaptive.step(&mut adaptive_lines).await;
            if adaptive_lines.len() < expected_hot.len() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        while legacy_lines.len() < expected_hot.len() && Instant::now() < deadline {
            let _ = legacy.step(&mut legacy_lines).await;
            if legacy_lines.len() < expected_hot.len() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        let mut got_adaptive: Vec<String> = adaptive_lines
            .iter()
            .map(|l| String::from_utf8(l.text.to_vec()).unwrap())
            .collect();
        let mut got_legacy: Vec<String> = legacy_lines
            .iter()
            .map(|l| String::from_utf8(l.text.to_vec()).unwrap())
            .collect();
        expected_hot.sort();
        got_adaptive.sort();
        got_legacy.sort();

        assert_eq!(got_adaptive, expected_hot);
        assert_eq!(got_legacy, expected_hot);
    }

    /// Verifies cross-directory symlink target watching AND atomic symlink rotation
    /// where the new target (`fp2`) is watched before the old target (`fp1`) is unwatched.
    #[cfg(unix)]
    #[test]
    fn notify_event_source_tracks_cross_directory_symlink_target_and_rotation() {
        let target_dir_1 = tempdir().unwrap();
        let target_dir_2 = tempdir().unwrap();
        let link_dir = tempdir().unwrap();

        let target_1 = target_dir_1.path().join("0.log");
        let target_2 = target_dir_2.path().join("1.log");
        let symlink_file = link_dir.path().join("pod_container.log");
        fs::write(&target_1, b"init1\n").unwrap();
        fs::write(&target_2, b"init2\n").unwrap();
        std::os::unix::fs::symlink(&target_1, &symlink_file).unwrap();

        let fp1 = FileFingerprint::DevInode(1, 10);
        let fp2 = FileFingerprint::DevInode(1, 20);

        let mut source = NotifyEventSource::new().expect("create notify watcher");
        source.watch_path(fp1, &symlink_file);
        assert_eq!(source.watched_dir_count(), 2);

        // Atomically retarget symlink to `target_2` and watch `fp2` BEFORE unwatching `fp1`.
        fs::remove_file(&symlink_file).unwrap();
        std::os::unix::fs::symlink(&target_2, &symlink_file).unwrap();
        source.watch_path(fp2, &symlink_file);
        assert_eq!(source.watched_dir_count(), 3);

        // Unwatching `fp1` must release `target_dir_1` while keeping `target_dir_2` active!
        source.unwatch_path(fp1);
        assert_eq!(source.watched_dir_count(), 2);
        assert_eq!(source.watched_file_count(), 1);

        let mut f = fs::OpenOptions::new().append(true).open(&target_2).unwrap();
        writeln!(f, "after_symlink_rotate").unwrap();
        f.flush().unwrap();

        let mut events = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            source.drain_events(&mut events);
            if events
                .iter()
                .any(|e| matches!(e, FsEvent::Modified(p) if p == &symlink_file))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(
            events
                .iter()
                .any(|e| matches!(e, FsEvent::Modified(p) if p == &symlink_file)),
            "expected Modified event remapped to symlink {symlink_file:?}, got {events:?}"
        );

        source.unwatch_path(fp2);
        assert_eq!(source.watched_dir_count(), 0);
        assert_eq!(source.watched_file_count(), 0);
    }
}
