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

    /// Notify the event source that `file_id` at `path` is now being watched.
    fn watch_path(&mut self, _file_id: FileFingerprint, _path: &Path) {}

    /// Notify the event source that `file_id` is no longer being watched.
    fn unwatch_path(&mut self, _file_id: FileFingerprint) {}

    /// Optional async notifier signaled whenever new events arrive, allowing
    /// `FileServer` to interrupt its idle backoff sleep immediately.
    fn wakeup_notifier(&self) -> Option<Arc<Notify>> {
        None
    }

    /// Number of active per-watcher registrations (used for DST leak invariants).
    fn watched_file_count(&self) -> usize {
        0
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

    /// Called when a watched file yielded one or more lines and hit `max_read_bytes`
    /// (still has unread data ready to read immediately on the next tick).
    fn on_file_read_more(
        &mut self,
        file_id: FileFingerprint,
        lines_read: usize,
        now: MonotonicInstant,
    );

    /// Called when a watched file reached EOF. `lines_read` is the number of lines
    /// successfully read before hitting EOF in this poll cycle.
    fn on_file_eof(&mut self, file_id: FileFingerprint, lines_read: usize, now: MonotonicInstant);

    /// Called when a watched file encountered a non-EOF read error.
    fn on_file_read_error(&mut self, _file_id: FileFingerprint, _now: MonotonicInstant) {}

    /// Called when discovery finds that a watched file is no longer findable
    /// (unlinked or moved outside `include`), ensuring it gets a final drain pass
    /// before `rotate_wait` can reap it.
    fn on_file_unfindable(&mut self, _file_id: FileFingerprint) {}

    /// Called when a watcher needs a fresh EOF verification (e.g. `remove_after`
    /// grace period has elapsed).
    fn on_file_needs_verification(&mut self, _file_id: FileFingerprint) {}

    /// Called when a file returned by `select_readable` was skipped (e.g., due
    /// to `oldest_first` stopping early) so event-driven schedulers can requeue it.
    fn on_file_skipped(&mut self, _file_id: FileFingerprint) {}

    /// Feed an external filesystem notification hint into the scheduler.
    fn on_fs_event(&mut self, event: &FsEvent, now: MonotonicInstant);

    /// Populate `out` with the ordered, deduplicated list of files eligible to be polled at `now`.
    fn select_readable(&mut self, now: MonotonicInstant, out: &mut Vec<FileFingerprint>);

    /// Earliest monotonic instant at which any parked file will become due for
    /// fallback polling (used to clamp outer backoff sleeps).
    fn next_wakeup(&self, _now: MonotonicInstant) -> Option<MonotonicInstant> {
        None
    }

    /// Number of queued internal timer entries (used by DST to assert bounded memory).
    fn pending_timer_count(&self) -> usize {
        0
    }
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

    fn on_file_read_more(
        &mut self,
        file_id: FileFingerprint,
        _lines_read: usize,
        now: MonotonicInstant,
    ) {
        if let Some(entry) = self.entries.get_mut(&file_id) {
            entry.reached_eof = false;
            entry.read_retry_delay = LEGACY_EOF_BACKOFF_MIN;
            entry.last_read_attempt = now;
            entry.last_read_success = now;
        }
    }

    fn on_file_eof(&mut self, file_id: FileFingerprint, lines_read: usize, now: MonotonicInstant) {
        if let Some(entry) = self.entries.get_mut(&file_id) {
            entry.last_read_attempt = now;
            if lines_read > 0 {
                entry.last_read_success = now;
                entry.read_retry_delay = LEGACY_EOF_BACKOFF_MIN;
            } else {
                entry.read_retry_delay = if entry.reached_eof {
                    std::cmp::min(
                        entry.read_retry_delay.saturating_mul(2),
                        LEGACY_EOF_BACKOFF_MAX,
                    )
                } else {
                    LEGACY_EOF_BACKOFF_MIN
                };
            }
            entry.reached_eof = true;
        }
    }

    fn on_file_read_error(&mut self, file_id: FileFingerprint, now: MonotonicInstant) {
        if let Some(entry) = self.entries.get_mut(&file_id) {
            entry.last_read_attempt = now;
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
            fallback_max: Duration::from_secs(10),
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
/// - Files signaled by [`FsEvent::Modified`] / [`FsEvent::Created`] / [`FsEvent::Overflow`]
///   (or newly added, renamed, unfindable, or hitting `max_read_bytes`) are promoted
///   immediately to the `dirty` queue and read on the very next tick with zero idle cliff.
/// - Idle files are parked in a min-heap (`O(k log N)` per tick, compacted to `O(N)` memory)
///   with exponential fallback backoff up to `fallback_max`.
/// - Both `dirty` and `fallback_heap` entries are validated against `(order, generation)`
///   so stale entries from prior EOFs or removed-and-re-added fingerprints can never cause
///   early polls or duplicate selection within a tick.
#[derive(Debug)]
pub struct AdaptiveEventScheduler {
    config: AdaptiveSchedulerConfig,
    next_order: u64,
    entries: HashMap<FileFingerprint, AdaptiveEntry>,
    path_to_fp: HashMap<PathBuf, FileFingerprint>,
    /// Files known or suspected to have unread data ready immediately: `(order, generation, file_id)`.
    dirty: Vec<(u64, u64, FileFingerprint)>,
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
            self.dirty.push((entry.order, entry.generation, file_id));
        }
    }

    fn schedule_fallback(
        &mut self,
        file_id: FileFingerprint,
        reset_delay: bool,
        now: MonotonicInstant,
    ) {
        if let Some(entry) = self.entries.get_mut(&file_id) {
            entry.dirty = false;
            entry.generation = entry.generation.wrapping_add(1);
            if reset_delay {
                entry.fallback_delay = self.config.fallback_min;
            }
            let due = now.checked_add(entry.fallback_delay).unwrap_or(now);
            entry.fallback_delay = std::cmp::min(
                entry.fallback_delay.saturating_mul(2),
                self.config.fallback_max,
            );
            self.fallback_heap
                .push(Reverse((due, entry.order, entry.generation, file_id)));
            self.maybe_compact_heap();
        }
    }

    /// Compacts stale/superseded heap entries so memory stays strictly `O(N)`.
    fn maybe_compact_heap(&mut self) {
        let limit = self.entries.len().saturating_mul(4).max(64);
        if self.fallback_heap.len() > limit {
            self.fallback_heap
                .retain(|Reverse((_, order, gen_id, fp))| {
                    self.entries
                        .get(fp)
                        .is_some_and(|e| e.order == *order && e.generation == *gen_id && !e.dirty)
                });
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
        self.dirty.push((order, 1, file_id));
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
        self.maybe_compact_heap();
    }

    fn on_file_read_more(
        &mut self,
        file_id: FileFingerprint,
        _lines_read: usize,
        _now: MonotonicInstant,
    ) {
        // File hit `max_read_bytes`; keep it hot for the very next tick.
        self.mark_dirty(file_id);
    }

    fn on_file_eof(&mut self, file_id: FileFingerprint, lines_read: usize, now: MonotonicInstant) {
        self.schedule_fallback(file_id, lines_read > 0, now);
    }

    fn on_file_read_error(&mut self, file_id: FileFingerprint, now: MonotonicInstant) {
        self.schedule_fallback(file_id, false, now);
    }

    fn on_file_unfindable(&mut self, file_id: FileFingerprint) {
        self.mark_dirty(file_id);
    }

    fn on_file_needs_verification(&mut self, file_id: FileFingerprint) {
        self.mark_dirty(file_id);
    }

    fn on_file_skipped(&mut self, file_id: FileFingerprint) {
        self.mark_dirty(file_id);
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

    fn select_readable(&mut self, now: MonotonicInstant, out: &mut Vec<FileFingerprint>) {
        // 1. Pop any fallback timers that have expired by `now`.
        while let Some(&Reverse((due, order, gen_id, file_id))) = self.fallback_heap.peek() {
            if due > now {
                break;
            }
            self.fallback_heap.pop();
            if let Some(entry) = self.entries.get_mut(&file_id)
                && entry.order == order
                && entry.generation == gen_id
                && !entry.dirty
            {
                entry.dirty = true;
                entry.generation = entry.generation.wrapping_add(1);
                self.dirty.push((entry.order, entry.generation, file_id));
            }
        }

        // 2. Drain `dirty`, validating each item against `(order, generation, dirty)` so
        //    stale entries can never trigger early polls or duplicate selection.
        let mut ready: Vec<(u64, FileFingerprint)> = Vec::new();
        for (order, gen_id, fp) in self.dirty.drain(..) {
            if let Some(entry) = self.entries.get_mut(&fp)
                && entry.order == order
                && entry.generation == gen_id
                && entry.dirty
            {
                entry.dirty = false;
                ready.push((order, fp));
            }
        }

        if self.config.oldest_first {
            ready.sort_by_key(|(order, _)| *order);
        }

        for (_, fp) in ready {
            out.push(fp);
        }
    }

    fn next_wakeup(&self, now: MonotonicInstant) -> Option<MonotonicInstant> {
        if self.entries.values().any(|e| e.dirty) {
            return Some(now);
        }
        self.fallback_heap.peek().map(|Reverse((due, ..))| *due)
    }

    fn pending_timer_count(&self) -> usize {
        self.fallback_heap.len()
    }
}
