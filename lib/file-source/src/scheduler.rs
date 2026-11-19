use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use file_source_common::{FileFingerprint, MonotonicInstant};
use indexmap::IndexMap;
use tokio::sync::Notify;

const LEGACY_EOF_BACKOFF_MIN: Duration = Duration::from_millis(1);
const LEGACY_EOF_BACKOFF_MAX: Duration = Duration::from_millis(250);
const LEGACY_IDLE_CLIFF: Duration = Duration::from_secs(10);

/// A hint from an OS filesystem notification backend (e.g. `inotify`, `kqueue`,
/// `ReadDirectoryChangesW`) or a simulated event source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    /// File contents at `path` may have been appended or modified.
    Modified(PathBuf),
    /// A new file may have been created at `path`.
    Created(PathBuf),
    /// A file at `path` was removed or renamed away.
    Removed(PathBuf),
    /// The OS event queue overflowed (e.g. `IN_Q_OVERFLOW`) or errored;
    /// schedulers must treat all watched files as potentially dirty.
    Overflow,
}

/// Non-blocking source of filesystem change hints.
pub trait FsEventSource: Send + 'static {
    /// Drain all pending filesystem events into `out` without blocking.
    fn drain_events(&mut self, out: &mut Vec<FsEvent>);

    /// Notify the event source that `path` is now being watched.
    fn watch_path(&mut self, _path: &Path) {}

    /// Notify the event source that `path` is no longer being watched.
    fn unwatch_path(&mut self, _path: &Path) {}

    /// Optional async notifier signaled whenever new events arrive, allowing
    /// `FileServer` to interrupt its idle backoff sleep immediately.
    fn wakeup_notifier(&self) -> Option<Arc<Notify>> {
        None
    }
}

/// Default no-op [`FsEventSource`] used when OS event notifications are disabled.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopFsEventSource;

impl FsEventSource for NoopFsEventSource {
    fn drain_events(&mut self, _out: &mut Vec<FsEvent>) {}
}

/// Policy deciding *which* watched files should be read on each `FileServer` tick
/// and in *what order*.
pub trait ReadScheduler: Send + 'static {
    /// Called when a new file begins being watched.
    /// `initial_ts` is the watcher's initial activity timestamp (derived from `mtime`).
    fn on_file_added(
        &mut self,
        file_id: FileFingerprint,
        path: &Path,
        initial_ts: MonotonicInstant,
        now: MonotonicInstant,
    );

    /// Called when a watched file's path changes (rename or fingerprint-collision switch).
    fn on_file_renamed(
        &mut self,
        file_id: FileFingerprint,
        old_path: &Path,
        new_path: &Path,
        now: MonotonicInstant,
    );

    /// Called when a watched file is unwatched/reaped.
    fn on_file_removed(&mut self, file_id: FileFingerprint);

    /// Called after a `read_line` attempt succeeds (`bytes_read > 0`) or completes a read cycle.
    fn on_file_read(&mut self, file_id: FileFingerprint, bytes_read: usize, now: MonotonicInstant);

    /// Called when a watched file hits EOF (no complete line available).
    fn on_file_eof(&mut self, file_id: FileFingerprint, now: MonotonicInstant);

    /// Feed an external filesystem notification hint into the scheduler.
    fn on_fs_event(&mut self, event: &FsEvent, now: MonotonicInstant);

    /// Called when a file returned by `select_readable` was skipped (e.g., due
    /// to `oldest_first` stopping early) so event-driven schedulers can requeue it.
    fn on_file_skipped(&mut self, _file_id: FileFingerprint) {}

    /// Populate `out` with the ordered list of files eligible to be polled at `now`.
    fn select_readable(&mut self, now: MonotonicInstant, out: &mut Vec<FileFingerprint>);
}

// ---------------------------------------------------------------------------
// LegacyPollingScheduler (1:1 preservation of original FileWatcher timing)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct LegacyEntry {
    reached_eof: bool,
    last_read_attempt: MonotonicInstant,
    last_read_success: MonotonicInstant,
    read_retry_delay: Duration,
}

impl LegacyEntry {
    fn should_read(&self, now: MonotonicInstant) -> bool {
        if self.reached_eof
            && now.saturating_duration_since(self.last_read_attempt) < self.read_retry_delay
        {
            return false;
        }

        now.saturating_duration_since(self.last_read_success) < LEGACY_IDLE_CLIFF
            || now.saturating_duration_since(self.last_read_attempt) > LEGACY_IDLE_CLIFF
    }
}

/// Preserves the exact original polling schedule of `FileServer` + `FileWatcher::should_read_at`:
/// scans all watched files in insertion order with 1ms..250ms EOF exponential backoff
/// and the 10-second idle poll cliff.
#[derive(Debug, Default)]
pub struct LegacyPollingScheduler {
    entries: IndexMap<FileFingerprint, LegacyEntry>,
}

impl ReadScheduler for LegacyPollingScheduler {
    fn on_file_added(
        &mut self,
        file_id: FileFingerprint,
        _path: &Path,
        initial_ts: MonotonicInstant,
        _now: MonotonicInstant,
    ) {
        self.entries.insert(
            file_id,
            LegacyEntry {
                reached_eof: false,
                last_read_attempt: initial_ts,
                last_read_success: initial_ts,
                read_retry_delay: LEGACY_EOF_BACKOFF_MIN,
            },
        );
    }

    fn on_file_renamed(
        &mut self,
        file_id: FileFingerprint,
        _old_path: &Path,
        _new_path: &Path,
        _now: MonotonicInstant,
    ) {
        if let Some(entry) = self.entries.get_mut(&file_id) {
            entry.reached_eof = false;
            entry.read_retry_delay = LEGACY_EOF_BACKOFF_MIN;
        }
    }

    fn on_file_removed(&mut self, file_id: FileFingerprint) {
        self.entries.shift_remove(&file_id);
    }

    fn on_file_read(
        &mut self,
        file_id: FileFingerprint,
        _bytes_read: usize,
        now: MonotonicInstant,
    ) {
        if let Some(entry) = self.entries.get_mut(&file_id) {
            entry.reached_eof = false;
            entry.read_retry_delay = LEGACY_EOF_BACKOFF_MIN;
            entry.last_read_attempt = now;
            entry.last_read_success = now;
        }
    }

    fn on_file_eof(&mut self, file_id: FileFingerprint, now: MonotonicInstant) {
        if let Some(entry) = self.entries.get_mut(&file_id) {
            entry.last_read_attempt = now;
            entry.read_retry_delay = if entry.reached_eof {
                std::cmp::min(
                    entry.read_retry_delay.saturating_mul(2),
                    LEGACY_EOF_BACKOFF_MAX,
                )
            } else {
                LEGACY_EOF_BACKOFF_MIN
            };
            entry.reached_eof = true;
        }
    }

    fn on_fs_event(&mut self, _event: &FsEvent, _now: MonotonicInstant) {
        // Legacy polling ignores external FS hints.
    }

    fn select_readable(&mut self, now: MonotonicInstant, out: &mut Vec<FileFingerprint>) {
        for (&file_id, entry) in &self.entries {
            if entry.should_read(now) {
                out.push(file_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AdaptiveEventScheduler (event-driven dirty queue + O(k log N) fallback heap)
// ---------------------------------------------------------------------------

/// Configuration for [`AdaptiveEventScheduler`].
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveSchedulerConfig {
    /// Initial fallback poll interval after hitting EOF.
    pub fallback_min: Duration,
    /// Maximum fallback poll interval for completely idle files (acts as a safety
    /// net when OS notifications are dropped, e.g. on NFS/SMB mounts).
    pub fallback_max: Duration,
    /// If true, eligible files are yielded in oldest-first (insertion) order.
    pub oldest_first: bool,
}

impl Default for AdaptiveSchedulerConfig {
    fn default() -> Self {
        Self {
            fallback_min: Duration::from_millis(250),
            fallback_max: Duration::from_secs(30),
            oldest_first: false,
        }
    }
}

#[derive(Debug)]
struct AdaptiveEntry {
    path: PathBuf,
    order: u64,
    generation: u64,
    dirty: bool,
    fallback_delay: Duration,
}

/// Event-driven + adaptive fallback [`ReadScheduler`].
///
/// - Files signaled by [`FsEvent::Modified`] / [`FsEvent::Created`] (or newly
///   added / actively producing bytes) are promoted immediately to the `dirty`
///   set and read on the very next tick with zero idle cliff.
/// - Idle files are parked in a min-heap (`O(k log N)` per tick) with exponential
///   fallback backoff up to `fallback_max`, avoiding `O(N)` scans and wasted
///   `read()` syscalls when watching thousands of quiet files.
/// - [`FsEvent::Overflow`] promotes all watched files back to `dirty` so missed
///   OS notifications never stall reads.
#[derive(Debug)]
pub struct AdaptiveEventScheduler {
    config: AdaptiveSchedulerConfig,
    next_order: u64,
    entries: HashMap<FileFingerprint, AdaptiveEntry>,
    path_to_fp: HashMap<PathBuf, FileFingerprint>,
    /// Files known or suspected to have unread data ready immediately.
    dirty: Vec<(u64, FileFingerprint)>,
    /// Min-heap of `(next_due_at, order, generation, file_id)` for fallback polling.
    fallback_heap: BinaryHeap<Reverse<(MonotonicInstant, u64, u64, FileFingerprint)>>,
}

impl AdaptiveEventScheduler {
    #[must_use]
    pub fn new(config: AdaptiveSchedulerConfig) -> Self {
        Self {
            config,
            next_order: 0,
            entries: HashMap::new(),
            path_to_fp: HashMap::new(),
            dirty: Vec::new(),
            fallback_heap: BinaryHeap::new(),
        }
    }

    fn mark_dirty(&mut self, file_id: FileFingerprint) {
        if let Some(entry) = self.entries.get_mut(&file_id)
            && !entry.dirty
        {
            entry.dirty = true;
            entry.generation = entry.generation.wrapping_add(1);
            entry.fallback_delay = self.config.fallback_min;
            self.dirty.push((entry.order, file_id));
        }
    }

    fn schedule_fallback(&mut self, file_id: FileFingerprint, now: MonotonicInstant) {
        if let Some(entry) = self.entries.get_mut(&file_id) {
            entry.dirty = false;
            entry.generation = entry.generation.wrapping_add(1);
            let due = now.checked_add(entry.fallback_delay).unwrap_or(now);
            entry.fallback_delay = std::cmp::min(
                entry.fallback_delay.saturating_mul(2),
                self.config.fallback_max,
            );
            self.fallback_heap
                .push(Reverse((due, entry.order, entry.generation, file_id)));
        }
    }
}

impl ReadScheduler for AdaptiveEventScheduler {
    fn on_file_added(
        &mut self,
        file_id: FileFingerprint,
        path: &Path,
        _initial_ts: MonotonicInstant,
        _now: MonotonicInstant,
    ) {
        let order = self.next_order;
        self.next_order += 1;
        let path_buf = path.to_path_buf();
        self.path_to_fp.insert(path_buf.clone(), file_id);
        self.entries.insert(
            file_id,
            AdaptiveEntry {
                path: path_buf,
                order,
                generation: 1,
                dirty: true,
                fallback_delay: self.config.fallback_min,
            },
        );
        self.dirty.push((order, file_id));
    }

    fn on_file_renamed(
        &mut self,
        file_id: FileFingerprint,
        old_path: &Path,
        new_path: &Path,
        _now: MonotonicInstant,
    ) {
        if self.path_to_fp.get(old_path) == Some(&file_id) {
            self.path_to_fp.remove(old_path);
        }
        let new_buf = new_path.to_path_buf();
        self.path_to_fp.insert(new_buf.clone(), file_id);
        if let Some(entry) = self.entries.get_mut(&file_id) {
            entry.path = new_buf;
        }
        self.mark_dirty(file_id);
    }

    fn on_file_removed(&mut self, file_id: FileFingerprint) {
        if let Some(entry) = self.entries.remove(&file_id)
            && self.path_to_fp.get(&entry.path) == Some(&file_id)
        {
            self.path_to_fp.remove(&entry.path);
        }
    }

    fn on_file_read(
        &mut self,
        file_id: FileFingerprint,
        _bytes_read: usize,
        _now: MonotonicInstant,
    ) {
        // File actively yielded bytes; keep it hot for the next tick.
        self.mark_dirty(file_id);
    }

    fn on_file_eof(&mut self, file_id: FileFingerprint, now: MonotonicInstant) {
        self.schedule_fallback(file_id, now);
    }

    fn on_fs_event(&mut self, event: &FsEvent, _now: MonotonicInstant) {
        match event {
            FsEvent::Modified(path) | FsEvent::Created(path) | FsEvent::Removed(path) => {
                if let Some(&file_id) = self.path_to_fp.get(path) {
                    self.mark_dirty(file_id);
                }
            }
            FsEvent::Overflow => {
                let all_ids: Vec<FileFingerprint> = self.entries.keys().copied().collect();
                for file_id in all_ids {
                    self.mark_dirty(file_id);
                }
            }
        }
    }

    fn on_file_skipped(&mut self, file_id: FileFingerprint) {
        self.mark_dirty(file_id);
    }

    fn select_readable(&mut self, now: MonotonicInstant, out: &mut Vec<FileFingerprint>) {
        // 1. Pop any fallback timers that have expired by `now`.
        while let Some(&Reverse((due, order, gen_id, file_id))) = self.fallback_heap.peek() {
            if due > now {
                break;
            }
            self.fallback_heap.pop();
            if let Some(entry) = self.entries.get_mut(&file_id)
                && entry.generation == gen_id
                && !entry.dirty
            {
                entry.dirty = true;
                self.dirty.push((order, file_id));
            }
        }

        // 2. Drain `dirty` (removing any stale entries for removed files).
        let mut ready: Vec<(u64, FileFingerprint)> = self
            .dirty
            .drain(..)
            .filter(|(_, fp)| self.entries.contains_key(fp))
            .collect();

        if self.config.oldest_first {
            ready.sort_by_key(|(order, _)| *order);
        }

        for (_, fp) in ready {
            if let Some(entry) = self.entries.get_mut(&fp) {
                entry.dirty = false;
            }
            out.push(fp);
        }
    }
}
