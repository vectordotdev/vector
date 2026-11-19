use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError, TrySendError},
    },
};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::sync::Notify;
use tracing::{debug, warn};

use crate::scheduler::{FsEvent, FsEventSource};

const EVENT_CHANNEL_CAPACITY: usize = 4096;

/// Production [`FsEventSource`] backed by `notify::RecommendedWatcher`
/// (`inotify` on Linux, `FSEvents` on macOS, `ReadDirectoryChangesW` on Windows).
///
/// Design properties:
/// - **Parent-directory ref-counted watches:** Watches unique parent directories
///   (`RecursiveMode::NonRecursive`) rather than individual files, keeping OS
///   watch-descriptor usage `O(directories)` instead of `O(files)` and surviving
///   atomic rename-replace rotations.
/// - **Bounded channel with overflow coalescing:** If the bounded event channel
///   fills up or `notify` reports an error / `need_rescan()`, an atomic
///   `overflowed` flag is set and drained as a single [`FsEvent::Overflow`].
/// - **Zero-latency async wakeup:** Signals an [`Arc<Notify>`] whenever events
///   arrive so `FileServer` can interrupt its idle backoff sleep immediately.
/// - **Canonical-path normalization:** Tracks canonical parent directory aliases
///   (e.g. macOS `/var` -> `/private/var`) so emitted paths match watched paths.
pub struct NotifyEventSource {
    watcher: RecommendedWatcher,
    rx: Receiver<FsEvent>,
    overflowed: Arc<AtomicBool>,
    wakeup: Arc<Notify>,
    /// Ref-count of watched files per parent directory.
    dir_refs: HashMap<PathBuf, usize>,
    /// Maps canonicalized parent directory -> original watched parent directory
    /// when they differ (e.g. symlinked parent dirs).
    canonical_to_watched: HashMap<PathBuf, Vec<PathBuf>>,
    /// Maps canonicalized target file path -> watched symlink path(s) when a
    /// watched file is a symlink pointing to a target (possibly in another directory).
    file_aliases: HashMap<PathBuf, Vec<PathBuf>>,
    /// Reverse index from watched symlink path -> canonical target file path for clean unwatch.
    watched_to_canonical_file: HashMap<PathBuf, PathBuf>,
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

        let cb_overflowed = Arc::clone(&overflowed);
        let cb_wakeup = Arc::clone(&wakeup);

        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let mut woke = false;
            match res {
                Ok(event) => {
                    if event.need_rescan() {
                        cb_overflowed.store(true, Ordering::Release);
                        woke = true;
                    } else if let Some(kind) = map_event_kind(event.kind) {
                        for path in event.paths {
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
            dir_refs: HashMap::new(),
            canonical_to_watched: HashMap::new(),
            file_aliases: HashMap::new(),
            watched_to_canonical_file: HashMap::new(),
        })
    }

    /// Returns the number of currently watched unique parent directories.
    #[must_use]
    pub fn watched_dir_count(&self) -> usize {
        self.dir_refs.len()
    }

    fn watch_dir(&mut self, dir: &Path) {
        let dir_buf = dir.to_path_buf();
        let count = self.dir_refs.entry(dir_buf.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            if let Err(error) = self.watcher.watch(dir, RecursiveMode::NonRecursive) {
                debug!(
                    message = "Failed to register OS directory watch; relying on fallback polling.",
                    dir = ?dir,
                    %error,
                );
            }
            if let Ok(canonical) = std::fs::canonicalize(dir)
                && canonical != dir_buf
            {
                self.canonical_to_watched
                    .entry(canonical)
                    .or_default()
                    .push(dir_buf);
            }
        }
    }

    fn unwatch_dir(&mut self, dir: &Path) {
        if let Some(count) = self.dir_refs.get_mut(dir) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.dir_refs.remove(dir);
                if let Err(error) = self.watcher.unwatch(dir) {
                    debug!(
                        message = "Failed to unregister OS directory watch.",
                        dir = ?dir,
                        %error,
                    );
                }
                self.canonical_to_watched.retain(|_, aliases| {
                    aliases.retain(|p| p != dir);
                    !aliases.is_empty()
                });
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

        // 1. Check direct canonical target -> symlink path aliases (e.g. Kubernetes pod logs).
        let mut emitted_alias = false;
        if let Some(symlink_paths) = self.file_aliases.get(path) {
            for sym_path in symlink_paths {
                out.push(Self::remap_event(&event, sym_path.clone()));
                emitted_alias = true;
            }
        }

        // 2. Check canonical parent dir -> watched parent dir aliases (e.g. macOS /private/var).
        if let (Some(parent), Some(file_name)) = (path.parent(), path.file_name())
            && let Some(aliases) = self.canonical_to_watched.get(parent)
        {
            for alias_parent in aliases {
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
            // Drain and discard buffered events; a single Overflow promotes all files.
            while self.rx.try_recv().is_ok() {}
            out.push(FsEvent::Overflow);
            return;
        }

        loop {
            match self.rx.try_recv() {
                Ok(event) => self.normalize_event(event, out),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    out.push(FsEvent::Overflow);
                    break;
                }
            }
        }
    }

    fn watch_path(&mut self, path: &Path) {
        if let Some(parent) = path.parent() {
            self.watch_dir(parent);
        }

        // If `path` is a symlink whose target resolves to a different file (potentially
        // in another directory, e.g. `/var/log/containers/*.log -> /var/log/pods/...`),
        // also watch the target's parent directory and map target events back to `path`.
        if let Ok(canonical_file) = std::fs::canonicalize(path)
            && canonical_file != path
        {
            if let Some(target_parent) = canonical_file.parent()
                && Some(target_parent) != path.parent()
            {
                self.watch_dir(target_parent);
            }
            self.file_aliases
                .entry(canonical_file.clone())
                .or_default()
                .push(path.to_path_buf());
            self.watched_to_canonical_file
                .insert(path.to_path_buf(), canonical_file);
        }
    }

    fn unwatch_path(&mut self, path: &Path) {
        if let Some(parent) = path.parent() {
            self.unwatch_dir(parent);
        }
        if let Some(canonical_file) = self.watched_to_canonical_file.remove(path) {
            if let Some(target_parent) = canonical_file.parent()
                && Some(target_parent) != path.parent()
            {
                self.unwatch_dir(target_parent);
            }
            if let Some(aliases) = self.file_aliases.get_mut(&canonical_file) {
                aliases.retain(|p| p != path);
                if aliases.is_empty() {
                    self.file_aliases.remove(&canonical_file);
                }
            }
        }
    }

    fn wakeup_notifier(&self) -> Option<Arc<Notify>> {
        Some(Arc::clone(&self.wakeup))
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
        Checkpointer, FingerprintStrategy, Fingerprinter, OsFileSystem, ReadFrom, SystemClock,
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
    fn notify_event_source_watches_parent_dir_and_receives_writes() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.log");
        let file_b = dir.path().join("b.log");
        fs::write(&file_a, b"init_a\n").unwrap();
        fs::write(&file_b, b"init_b\n").unwrap();

        let mut source = NotifyEventSource::new().expect("create notify watcher");
        source.watch_path(&file_a);
        source.watch_path(&file_b);
        // Two files in the same directory -> only 1 OS directory watch!
        assert_eq!(source.watched_dir_count(), 1);

        // Append to `file_a` and wait briefly for OS event delivery.
        let mut f = fs::OpenOptions::new().append(true).open(&file_a).unwrap();
        writeln!(f, "hello_notify").unwrap();
        f.flush().unwrap();

        let mut events = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
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
        source.unwatch_path(&file_a);
        assert_eq!(source.watched_dir_count(), 1);
        source.unwatch_path(&file_b);
        assert_eq!(source.watched_dir_count(), 0);
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

        // Initial seed drain
        let mut legacy_lines: Vec<Line> = Vec::new();
        let mut adaptive_lines: Vec<Line> = Vec::new();
        let _ = legacy.step(&mut legacy_lines).await;
        let _ = adaptive.step(&mut adaptive_lines).await;
        assert_eq!(legacy_lines.len(), idle_count + 2);
        assert_eq!(adaptive_lines.len(), idle_count + 2);
        legacy_lines.clear();
        adaptive_lines.clear();

        // Append 20 lines to each hot file and verify both deliver identical lines,
        // with `NotifyEventSource` waking `AdaptiveEventScheduler` immediately.
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

        // Wait for OS notify wakeup (bounded by 500ms) and step adaptive.
        assert!(
            tokio::time::timeout(Duration::from_millis(500), wakeup.notified())
                .await
                .is_ok(),
            "expected NotifyEventSource to wake within 500ms"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
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

    #[cfg(unix)]
    #[test]
    fn notify_event_source_tracks_cross_directory_symlink_target() {
        let target_dir = tempdir().unwrap();
        let link_dir = tempdir().unwrap();

        let target_file = target_dir.path().join("0.log");
        let symlink_file = link_dir.path().join("pod_container.log");
        fs::write(&target_file, b"init\n").unwrap();
        std::os::unix::fs::symlink(&target_file, &symlink_file).unwrap();

        let mut source = NotifyEventSource::new().expect("create notify watcher");
        source.watch_path(&symlink_file);
        // Watches both the symlink's directory AND the target's directory.
        assert_eq!(source.watched_dir_count(), 2);

        // Append directly to the target file (in a different directory!).
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&target_file)
            .unwrap();
        writeln!(f, "pod_log_line").unwrap();
        f.flush().unwrap();

        let mut events = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
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

        // Unwatching the symlink cleans up both directory watches.
        source.unwatch_path(&symlink_file);
        assert_eq!(source.watched_dir_count(), 0);
    }
}
