use std::{
    cmp,
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{self, Duration},
};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use file_source_common::{
    FileFingerprint, FilePosition, FileSourceInternalEvents, Fingerprinter, OwnerGeneration,
    PrefixWanted, ReadFrom,
    checkpointer::{Checkpointer, CheckpointsView},
};
use futures::{
    Future, Sink, SinkExt,
    future::{Either, select},
};
use futures_util::future::join_all;
use indexmap::IndexMap;
use tokio::{
    fs::{self, remove_file},
    task::{Id, JoinSet},
    time::sleep,
};

use tracing::{debug, error, info, trace, warn};

use crate::{
    file_watcher::{FileWatcher, RawLineResult, identify_event_paths, identify_paths_in_tree},
    notify_watcher::{NotifyDiscovery, NotifyMessage},
    paths_provider::PathsProvider,
};

/// How long to briefly wait for more OS-level file events to arrive, after the first one, before
/// running a reconciliation pass -- so a burst of events (e.g. an editor doing several small
/// writes) collapses into one pass instead of one per event. Deliberately a small, fixed
/// constant rather than derived from user-facing config: `glob_minimum_cooldown`/
/// `reconcile_interval` control how *rarely* discovery runs, which is the opposite of what this
/// value is for. In particular, a user who sets a large `glob_minimum_cooldown`/
/// `reconcile_interval` specifically to make the (expensive) backstop reconciliation pass rare
/// under `FileDiscoveryMode::Notify` must not have that same value silently become the debounce
/// window and delay every single notify-driven discovery by that same large amount.
const NOTIFY_EVENT_DEBOUNCE: Duration = Duration::from_millis(50);

/// How often the background checkpoint-writer task persists checkpoints to disk. Kept independent
/// of `glob_minimum_cooldown`, which is documented as ignored under `Notify` mode -- otherwise a
/// large `glob_minimum_cooldown` would silently also throttle checkpoint persistence.
const CHECKPOINT_WRITE_INTERVAL: Duration = Duration::from_secs(1);

/// Minimum delay between retries after an idle-file removal fails. This prevents a persistent
/// filesystem error from turning the removal deadline into a zero-duration busy loop.
const IDLE_REMOVAL_RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// Minimum time between two full glob+fingerprint reconciliation passes (`discover`) triggered by
/// notify events. Without this, a file under sustained writes would trigger a full re-glob on
/// every `NOTIFY_EVENT_DEBOUNCE` window indefinitely. Doesn't delay reads of already-tracked
/// files (those run every main-loop iteration regardless), but does delay
/// `FileWatcher::mark_ready_to_read`'s nudge by up to this much, since that only happens inside
/// `discover`.
const MIN_NOTIFY_DISCOVERY_INTERVAL: Duration = Duration::from_millis(500);

/// Above this many distinct paths accumulated from notify events since the last reconciliation
/// pass, stop tracking them individually and treat the wakeup as "something changed".
const NOTIFY_WAKEUP_PATH_LIMIT: usize = 1024;

/// Keep rename candidates separately from ordinary change paths. A large write burst must not
/// discard the paths needed to recover an idle rotated file.
const NOTIFY_RENAME_PATH_LIMIT: usize = 1024;

/// Accumulates, between reconciliation passes, which specific paths (if known) notify events have
/// named -- so that `discover`'s "nudge this watcher past its read-pacing timers" step (see
/// `FileWatcher::mark_ready_to_read`) only touches watchers a concrete event actually named,
/// instead of every currently-tracked watcher on every single notify event regardless of which
/// path it was about. The latter is an O(N) cost (N = number of tracked files) per event, which
/// under a large `include` glob turns "one file got appended to" into "redundantly reconsider
/// every other file's read pacing too."
#[derive(Debug, Default)]
struct NotifyWakeup {
    state: NotifyWakeupState,
    /// Canonical forms of named event paths. This is populated once per notify batch so a
    /// watcher whose glob path contains a symlink can still be nudged without canonicalizing
    /// every tracked file on every discovery pass.
    canonical_paths: HashSet<PathBuf>,
    /// Raw paths awaiting their first canonicalization attempt. Keeping only this delta makes
    /// repeated notify events O(new paths) while still retrying paths that temporarily disappear
    /// during a rename.
    canonical_paths_pending: HashSet<PathBuf>,
    /// Paths whose first canonicalization attempt failed. These are retried once after the current
    /// notify debounce batch, rather than once for every event in that batch.
    canonical_paths_retry: HashSet<PathBuf>,
    rename_paths: HashSet<PathBuf>,
    rename_paths_incomplete: bool,
}

#[derive(Debug, Default)]
enum NotifyWakeupState {
    #[default]
    None,
    Paths(HashSet<PathBuf>),
    All,
}

impl NotifyWakeup {
    fn is_pending(&self) -> bool {
        !matches!(&self.state, NotifyWakeupState::None)
    }

    fn add_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let paths: Vec<PathBuf> = paths.into_iter().collect();
        // A pathless notify event is coarse by definition. Preserve that meaning even if a
        // named event arrives before the pending wakeup is reconciled.
        if paths.is_empty() {
            self.mark_all();
            return;
        }
        let mut newly_added = HashSet::new();

        match &mut self.state {
            NotifyWakeupState::All => {}
            NotifyWakeupState::None => {
                let set: HashSet<PathBuf> = paths.into_iter().collect();
                self.state = if set.len() > NOTIFY_WAKEUP_PATH_LIMIT {
                    NotifyWakeupState::All
                } else {
                    newly_added.extend(set.iter().cloned());
                    NotifyWakeupState::Paths(set)
                };
            }
            NotifyWakeupState::Paths(existing) => {
                for path in paths {
                    if existing.insert(path.clone()) {
                        newly_added.insert(path);
                    }
                }
                if existing.len() > NOTIFY_WAKEUP_PATH_LIMIT {
                    self.state = NotifyWakeupState::All;
                }
            }
        }

        if matches!(self.state, NotifyWakeupState::Paths(_)) {
            self.canonical_paths_pending.extend(newly_added);
        } else {
            self.canonical_paths.clear();
            self.canonical_paths_pending.clear();
            self.canonical_paths_retry.clear();
        }
    }

    /// Resolve named event paths without blocking the async file-server task. Notify callbacks
    /// run outside the runtime, so filesystem work must be offloaded before matching symlinked
    /// watcher paths during reconciliation.
    async fn resolve_canonical_paths(&mut self) {
        let paths: Vec<PathBuf> = match &self.state {
            NotifyWakeupState::Paths(_) => self.canonical_paths_pending.iter().cloned().collect(),
            NotifyWakeupState::None | NotifyWakeupState::All => return,
        };
        if paths.is_empty() {
            return;
        }
        self.canonical_paths_pending.clear();

        let canonical_paths = join_all(paths.iter().map(fs::canonicalize))
            .await
            .into_iter();
        for (path, canonical) in paths.into_iter().zip(canonical_paths) {
            if let Ok(canonical) = canonical {
                self.canonical_paths.insert(canonical);
                self.canonical_paths_retry.remove(&path);
            } else {
                self.canonical_paths_retry.insert(path);
            }
        }
        if self.canonical_paths.len() > NOTIFY_WAKEUP_PATH_LIMIT {
            self.mark_all();
        }
    }

    /// Retry paths that were missing during the first canonicalization attempt once the current
    /// notify debounce batch is complete. A rename can make one of these paths available again,
    /// but retrying it for every event in the batch makes a missing path increasingly expensive.
    async fn retry_canonical_paths(&mut self) {
        let paths: Vec<PathBuf> = match &self.state {
            NotifyWakeupState::Paths(_) => self.canonical_paths_retry.iter().cloned().collect(),
            NotifyWakeupState::None | NotifyWakeupState::All => return,
        };
        if paths.is_empty() {
            return;
        }

        let canonical_paths = join_all(paths.iter().map(fs::canonicalize))
            .await
            .into_iter();
        for (path, canonical) in paths.into_iter().zip(canonical_paths) {
            if let Ok(canonical) = canonical {
                self.canonical_paths.insert(canonical);
                self.canonical_paths_retry.remove(&path);
            }
        }
        if self.canonical_paths.len() > NOTIFY_WAKEUP_PATH_LIMIT {
            self.mark_all();
        }
    }

    fn add_removed_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let paths: Vec<PathBuf> = paths.into_iter().collect();
        self.add_paths(paths.iter().cloned());
        self.add_rename_candidates(paths);
    }

    fn add_created_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let paths: Vec<PathBuf> = paths.into_iter().collect();
        self.add_paths(paths.iter().cloned());
        self.add_rename_candidates(paths);
    }

    fn add_rename_candidates(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            if self.rename_paths.len() < NOTIFY_RENAME_PATH_LIMIT
                || self.rename_paths.contains(&path)
            {
                self.rename_paths.insert(path);
            } else {
                self.rename_paths_incomplete = true;
            }
        }
    }

    fn mark_all(&mut self) {
        self.state = NotifyWakeupState::All;
        self.canonical_paths.clear();
        self.canonical_paths_pending.clear();
        self.canonical_paths_retry.clear();
    }

    fn take(&mut self) -> NotifyWakeup {
        std::mem::take(self)
    }

    /// Whether `path` should have its watcher nudged past its own read-pacing timers (see
    /// `FileWatcher::mark_ready_to_read`) for this reconciliation pass.
    fn names(&self, path: &Path, canonical_path: Option<&Path>) -> bool {
        match &self.state {
            NotifyWakeupState::None => false,
            NotifyWakeupState::All => true,
            NotifyWakeupState::Paths(paths) => {
                paths.contains(path)
                    || canonical_path.is_some_and(|path| self.canonical_paths.contains(path))
            }
        }
    }

    /// Return paths from an ordinary change event. These events are safe to process directly:
    /// create/remove/rename events retain `rename_paths` and use the full reconciliation path,
    /// while a plain modification only needs to fingerprint the files it named.
    fn targeted_change_paths(&self) -> Option<&HashSet<PathBuf>> {
        if !self.rename_paths.is_empty() {
            return None;
        }
        match &self.state {
            NotifyWakeupState::Paths(paths) if !paths.is_empty() => Some(paths),
            NotifyWakeupState::None | NotifyWakeupState::All | NotifyWakeupState::Paths(_) => None,
        }
    }

    fn has_specific_paths(&self) -> bool {
        matches!(&self.state, NotifyWakeupState::Paths(paths) if !paths.is_empty())
    }

    fn named_paths(&self) -> Option<&HashSet<PathBuf>> {
        (!self.rename_paths.is_empty()).then_some(&self.rename_paths)
    }

    fn requires_broad_rename_scan(&self) -> bool {
        self.rename_paths_incomplete
            // Raw rename paths are only candidates. Until one resolves to this watcher's inode,
            // the path may already be gone (RenameFrom/Remove), so a tree scan is still needed.
            || !self.rename_paths.is_empty()
            || matches!(
                &self.state,
                NotifyWakeupState::None | NotifyWakeupState::All
            )
            || matches!(&self.state, NotifyWakeupState::Paths(paths) if paths.is_empty())
    }

    fn rename_paths_incomplete(&self) -> bool {
        self.rename_paths_incomplete
    }
}

/// Whether a watcher that `discover`'s glob/fingerprint pass just marked unfindable should be
/// reaped (`set_dead`) on this cycle.
///
/// An `Active` watcher left unfindable keeps getting read (and, on EOF, marked dead) every cycle
/// regardless of `rotate_wait`, so `rotate_wait` only matters there as a grace period against a
/// premature `unwatch`; this function's `false` result for such a watcher (until `rotate_wait`
/// elapses) preserves that pre-existing behavior unchanged.
///
/// An `Idle` watcher is normally not read while unfindable, because reopening its old path could
/// attach the stale checkpoint to a replacement file. It is instead given one full
/// `discovery_interval` to be fingerprint-matched at a new in-glob path. If the watcher has been
/// located by identity at a path outside the glob, it follows the old `rotate_wait` grace period
/// just like an `Active` watcher; `poll_idle_watchers` continues checking that known path.
/// If rename candidates were truncated by a burst, defer this short idle grace period for a bounded
/// recovery window, because the missing destination may still be found by a later pass. The window
/// is managed by the caller and must not be extended by unrelated unfindable watchers.
fn should_reap_unfindable_watcher(
    is_idle: bool,
    path_outside_glob: bool,
    unfindable_for: Duration,
    discovery_interval: Duration,
    rotate_wait: Duration,
    rename_recovery_protected: bool,
) -> bool {
    (is_idle
        && !path_outside_glob
        && !rename_recovery_protected
        && unfindable_for > discovery_interval)
        || unfindable_for > rotate_wait
}

/// Move a tracked watcher from `old_key` to `new_key`, carrying its checkpoint with it.
///
/// A file's fingerprint is not stable: an in-place rewrite (`copytruncate`, or an application that
/// truncates and rewrites its own log) changes the first line that `FirstLinesChecksum` hashes. The
/// watcher is still the right reader for that path, but its `fp_map` key -- which is what
/// `Line::file_id` carries downstream and what the checkpointer persists under -- now names a
/// fingerprint the file no longer has. Left alone, two things go wrong: emitted lines are
/// checkpointed under an identity that does not match the file, and the next full pass does not
/// recognise the new fingerprint as tracked and starts a *second* watcher on the same path.
///
/// Returns `false` when `new_key` is already occupied, which means some other watcher legitimately
/// owns that fingerprint (two files can share one, e.g. identical first lines); rekeying would
/// evict it, so the caller must leave the collision to the full pass instead.
/// The key a watcher for `path` is currently filed under, if any.
///
/// Extracted so the `IndexMap` iterator is dropped before the caller awaits anything: that iterator
/// is not `Send`, and holding it across an `await` makes `FileServer::run`'s future non-`Send`.
/// Resolves a discovered path to the watcher already tracking it, for the full pass.
///
/// Needed when a tracked file's fingerprint changes (an in-place rewrite), so `fp_map`'s own lookup
/// misses. Scanning `fp_map` for each such path is quadratic in the number of rewritten files:
/// measured at 185ms for 20k, string comparison alone, excluding the two `absolutize` allocations
/// `matches_path` performs per comparison.
///
/// Built on the first miss, then maintained in place. Rebuilding after each mutation instead would be
/// the same O(N^2) with more hashing, since a burst mutates on every path.
#[derive(Default)]
struct TrackedPathIndex {
    keys_by_path: Option<HashMap<PathBuf, FileFingerprint>>,
    /// The spellings filed under each key, so a rekey touches only its own entries.
    ///
    /// Without it, rekeying scans every value: measured at 935ms for 20k rewrites, *worse* than the
    /// 185ms linear search this type replaces.
    paths_by_key: HashMap<FileFingerprint, Vec<PathBuf>>,
}

impl TrackedPathIndex {
    fn key_for_path(
        &mut self,
        fp_map: &IndexMap<FileFingerprint, FileWatcher>,
        path: &Path,
        canonical_path: Option<&Path>,
        cwd: Option<&Path>,
    ) -> Option<FileFingerprint> {
        if self.keys_by_path.is_none() {
            let mut index: HashMap<PathBuf, FileFingerprint> =
                HashMap::with_capacity(fp_map.len() * 2);
            let mut by_key: HashMap<FileFingerprint, Vec<PathBuf>> =
                HashMap::with_capacity(fp_map.len());
            for (&key, watcher) in fp_map {
                let mut record = |spelling: PathBuf| {
                    if index.entry(spelling.clone()).or_insert(key) == &key {
                        by_key.entry(key).or_default().push(spelling);
                    }
                };
                record(crate::absolutize(&watcher.path, cwd));
                if let Some(canonical) = watcher.canonical_path() {
                    record(canonical.to_path_buf());
                }
            }
            self.keys_by_path = Some(index);
            self.paths_by_key = by_key;
        }
        let keys_by_path = self
            .keys_by_path
            .as_ref()
            .expect("just built above if it was absent");
        // Both spellings, as the scan did: a candidate can itself be an alias (overlapping includes,
        // or a glob through a symlink), and missing that adds a second reader for one inode.
        keys_by_path
            .get(&crate::absolutize(path, cwd))
            .or_else(|| canonical_path.and_then(|canonical| keys_by_path.get(canonical)))
            .copied()
    }

    /// A watcher moved from `old_key` to `new_key`; its paths are unchanged.
    fn rekeyed(&mut self, old_key: FileFingerprint, new_key: FileFingerprint) {
        let Some(index) = self.keys_by_path.as_mut() else {
            return;
        };
        let Some(spellings) = self.paths_by_key.remove(&old_key) else {
            return;
        };
        for spelling in &spellings {
            if let Some(key) = index.get_mut(spelling) {
                *key = new_key;
            }
        }
        self.paths_by_key.insert(new_key, spellings);
    }

    /// A watcher's set of paths changed, or one was added or removed. Cheaper to rebuild on the next
    /// miss than to reconcile every alias, and these are rare next to rewrites.
    fn paths_changed(&mut self) {
        self.keys_by_path = None;
        self.paths_by_key.clear();
    }
}

fn rekey_watcher(
    fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
    checkpoints: &CheckpointsView,
    old_key: FileFingerprint,
    new_key: FileFingerprint,
) -> bool {
    rekey_watcher_with_drain(fp_map, checkpoints, old_key, new_key, None)
}

fn rekey_watcher_with_drain(
    fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
    checkpoints: &CheckpointsView,
    old_key: FileFingerprint,
    new_key: FileFingerprint,
    drained_checkpoint: Option<(FilePosition, OwnerGeneration)>,
) -> bool {
    if old_key == new_key {
        return true;
    }
    if fp_map.contains_key(&new_key) {
        return false;
    }
    // `fp_map`'s iteration order is read priority under `oldest_first`: startup sorts by creation
    // time and new watchers append. Removing and re-inserting would move a rewritten older file to
    // the tail, letting a newer one drain first, so the entry goes back at the index it held.
    let Some(position) = fp_map.get_index_of(&old_key) else {
        return false;
    };
    let Some(mut watcher) = fp_map.shift_remove(&old_key) else {
        return false;
    };
    // Asks the watcher whether its reader was repositioned onto different content, rather than
    // inferring it from a zero offset: a watcher that simply has not read anything yet also sits at
    // zero, and resetting its checkpoint would discard a resumed position.
    let restarted = watcher.take_reader_restarted();
    // A new owner for a new identity. Lines already in flight keep the old generation. For a plain
    // rekey their acknowledgements find the old key gone; a draining rekey retains that key as a
    // reaped checkpoint until those acknowledgements have been accepted.
    let generation = watcher.take_new_generation();
    fp_map.shift_insert(position, new_key, watcher);
    // Carries the persisted position and the modified/removed bookkeeping onto the new identity.
    // During a rotation, retain the old entry as a reaped checkpoint so acknowledgements from the
    // bounded drain cannot fall into the gap between removing the old key and registering it again.
    if let Some((resume_position, drained_generation)) = drained_checkpoint {
        checkpoints.update_key_and_register_reaped(
            old_key,
            new_key,
            generation,
            drained_generation,
            resume_position,
        );
    } else {
        checkpoints.update_key_and_get_position(old_key, new_key, generation);
    }
    if restarted {
        // The reader was restarted at zero (an in-place rewrite), so the pre-rewrite offset must not
        // survive: a restart would resume past the start of the rewritten file and skip its opening
        // content. An appended-to file keeps its offset, and its checkpoint with it.
        checkpoints.register(new_key, 0, generation);
    }
    true
}

/// Whether any component of `path` is a symlink.
///
/// Asked of the filesystem, because comparing a path against its canonical form does not answer it:
/// canonicalization also rewrites paths with no symlink involved (a Windows 8.3 short name becomes
/// the long name under a `\\?\` prefix; macOS `/var` becomes `/private/var`), and treating that as a
/// symlink forces a full glob pass for every unrelated file in a watched directory.
async fn path_contains_symlink(path: &Path) -> bool {
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor)
            .await
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return true;
        }
    }
    false
}

/// Whether `canonical_path` can be reached from `directory` through a symlink at most `depth` levels
/// below it.
///
/// A glob's wildcard component can traverse a symlink (`<root>/*/*.log` where `<root>/linked` points
/// elsewhere), which no comparison of the pattern's literal prefix can detect.
///
/// `Some(false)` only when the tree was fully explored within `depth`. `None` when the bound stopped
/// the walk with directories left, since a link may sit deeper -- the caller must then defer to the
/// full glob pass rather than treat the event as excluded. Reached only for an untracked event path
/// that matched no spelling, which is what keeps the walk off the hot path.
async fn path_reachable_through_child_link(
    directory: &Path,
    canonical_path: &Path,
    depth: usize,
) -> Option<bool> {
    // Breadth-first to `depth`, so a recursive `**` pattern finds a link nested below a real
    // subdirectory while a single-`*` pattern still reads one level. Iterative rather than
    // recursive: an `async fn` that awaits itself needs boxing, and the bound is the point.
    let mut frontier = vec![directory.to_path_buf()];
    for _ in 0..depth {
        let mut next = Vec::new();
        // Any traversal error makes the answer undecided rather than negative: a directory that is
        // momentarily unreadable would otherwise leave `next` empty and be reported as "fully
        // explored, no link", so the caller would skip the glob pass for a file it should have found.
        for directory in &frontier {
            let Ok(mut entries) = fs::read_dir(directory).await else {
                return None;
            };
            loop {
                let entry = match entries.next_entry().await {
                    Ok(Some(entry)) => entry,
                    Ok(None) => break,
                    Err(_) => return None,
                };
                let Ok(file_type) = entry.file_type().await else {
                    return None;
                };
                if file_type.is_symlink() {
                    let Ok(target) = fs::canonicalize(entry.path()).await else {
                        return None;
                    };
                    if canonical_path.starts_with(&target) {
                        return Some(true);
                    }
                    // A link to a directory can hold further links, and the path may be reached
                    // through one of those. Descending would need cycle protection, so the answer is
                    // undecided instead -- the full pass settles it.
                    if fs::metadata(&target)
                        .await
                        .is_ok_and(|metadata| metadata.is_dir())
                    {
                        return None;
                    }
                } else if file_type.is_dir() {
                    // A real subdirectory cannot itself explain the path (the prefix comparison
                    // already covers that), but a link may sit under it.
                    next.push(entry.path());
                }
            }
        }
        if next.is_empty() {
            // The tree under `directory` is fully explored: a definite "no link reaches this path".
            return Some(false);
        }
        frontier = next;
    }
    // The cap stopped the walk with directories still unexplored, so a link may sit deeper. Undecided,
    // not "no": reporting `false` here made the caller treat the event as excluded and skip the glob
    // pass, leaving a file undiscovered until the backstop and losing a short-lived one outright.
    None
}

/// How deep [`path_reachable_through_child_link`] should descend for `pattern`.
///
/// A `**` component matches any number of directories, so a link can sit arbitrarily deep; the walk
/// is capped rather than unbounded, because this runs per event and an unbounded descent is the O(N)
/// tree walk the targeted pass exists to avoid. Anything else needs only as many levels as the
/// pattern has wildcard components. Hitting the cap is not a negative answer -- the walk reports it as
/// undecided, so the event still reaches the full pass.
fn symlink_search_depth(pattern: &Path, literal_root: &Path) -> usize {
    /// Enough to cover the nesting real log layouts use (`<root>/<service>/<pod>/<container>`)
    /// without letting one event walk a deep tree.
    const RECURSIVE_DEPTH_CAP: usize = 4;

    let wildcard_components = pattern
        .strip_prefix(literal_root)
        .unwrap_or(pattern)
        .components()
        .count();
    if pattern
        .components()
        .any(|component| component.as_os_str() == "**")
    {
        wildcard_components.max(RECURSIVE_DEPTH_CAP)
    } else {
        // One level per wildcard component, and at least one: the component adjacent to the literal
        // root is where a link most often sits.
        wildcard_components.max(1)
    }
}

fn idle_watcher_can_be_polled(
    file_findable: bool,
    path_outside_glob: bool,
    path_has_tracked_identity: bool,
) -> bool {
    file_findable || (path_outside_glob && path_has_tracked_identity)
}

fn update_rename_recovery_deadline(
    deadline: Option<time::Instant>,
    incomplete: bool,
    now: time::Instant,
    discovery_interval: Duration,
) -> Option<time::Instant> {
    // Keep an already-live window unchanged. This makes repeated observations of one truncated
    // burst idempotent; only an incomplete burst observed after expiry starts a new window.
    if incomplete && deadline.is_none_or(|deadline| deadline <= now) {
        Some(
            now.checked_add(discovery_interval.saturating_mul(2))
                .unwrap_or(now),
        )
    } else {
        deadline
    }
}

/// Salvage `watcher`'s final unterminated line, if any, into `lines`. Call this right before
/// every `set_dead()` that doesn't already go through `read_line` first (which has its own flush
/// for the `Active` case) -- otherwise a trailing record with no delimiter is lost for good.
fn salvage_final_partial_line(
    watcher: &mut FileWatcher,
    file_id: FileFingerprint,
    lines: &mut Vec<Line>,
) {
    let Some(line) = watcher.take_final_partial_line() else {
        return;
    };
    let end_offset = line.offset + line.bytes.len() as u64;
    lines.push(Line {
        text: line.bytes,
        filename: watcher.path.to_str().expect("not a valid path").to_owned(),
        file_id,
        generation: watcher.generation(),
        start_offset: line.offset,
        end_offset,
    });
}

/// The result of a bounded rotation drain.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum DrainOutcome {
    /// The old inode reached EOF and the watcher was pointed at `replacement`.
    Repointed {
        bytes_read: usize,
        old_position: FilePosition,
        old_generation: OwnerGeneration,
    },
    /// The read budget was exhausted before EOF. The watcher remains on the old inode so the
    /// caller can continue draining it in a later pass.
    LimitReached { bytes_read: usize },
}

/// Point a watcher at the file that replaced its own, reading out everything left on the old one
/// first, subject to `max_read_bytes`.
///
/// Rotation replaces the file at a tracked path, and reopening there abandons the inode the reader
/// still holds: its unread records, and the unterminated one in its buffer, would be lost. Both are
/// emitted under the fingerprint the watcher still has, so they are checkpointed against the file
/// they came from rather than the one about to take its place.
///
/// EOF here means "nothing more to read now"; a writer still holding the old descriptor can append
/// afterwards, and those bytes are beyond recovery once the reader moves. If the limit is reached
/// first, the old descriptor and fingerprint are retained and the caller must invoke this function
/// again before repointing.
async fn drain_and_repoint(
    watcher: &mut FileWatcher,
    file_id: FileFingerprint,
    replacement: PathBuf,
    lines: &mut Vec<Line>,
    max_read_bytes: usize,
) -> std::io::Result<DrainOutcome> {
    // The pass marked every watcher unfindable before discovery, and `read_line` reads that at EOF as
    // "the file was deleted" and kills the watcher -- which would then be reaped straight after being
    // repointed. This file was just fingerprinted, so saying it was found is simply true.
    watcher.mark_found();
    let old_generation = watcher.generation();
    let mut bytes_read: usize = 0;
    loop {
        match watcher.read_line().await {
            Ok(RawLineResult {
                raw_line: Some(line),
                ..
            }) => {
                bytes_read = bytes_read.saturating_add(line.bytes.len());
                lines.push(Line {
                    text: line.bytes,
                    filename: watcher.path.to_str().expect("not a valid path").to_owned(),
                    file_id,
                    generation: watcher.generation(),
                    start_offset: line.offset,
                    end_offset: watcher.get_file_position(),
                });
                // Match the normal read loop's per-file budget. In particular, do not probe EOF
                // after this line: retaining the watcher on the old descriptor is what makes the
                // next pass able to continue without keeping the whole tail in `lines`.
                if bytes_read > max_read_bytes {
                    return Ok(DrainOutcome::LimitReached { bytes_read });
                }
            }
            Ok(_) => break,
            // Not EOF: the file may still hold records this reader has not seen. Moving on would
            // drop the descriptor along with them, so leave the watcher where it is and let the next
            // pass try again -- the reader keeps its offset, and the replacement is still there.
            Err(error) => {
                watcher.prepare_for_discovery();
                return Err(error);
            }
        }
    }
    salvage_final_partial_line(watcher, file_id, lines);
    let old_position = watcher.get_file_position();

    let repointed = watcher.update_path(replacement).await;
    if repointed.is_err() {
        // Nothing was found after all -- the replacement went away in the window between
        // fingerprinting it and opening it. Put the watcher back where the pass left it, or it stays
        // "found" on the inode it no longer describes and `remove_after` unlinks whatever now
        // occupies its old path.
        watcher.prepare_for_discovery();
    }
    repointed.map(|()| DrainOutcome::Repointed {
        bytes_read,
        old_position,
        old_generation,
    })
}

/// The outcome of one discovery pass. Notify discovery remains an implementation detail of the
/// caller, while a bounded rotation drain needs to tell the main loop to retry reconciliation
/// immediately rather than waiting for the normal polling interval.
#[derive(Debug, PartialEq, Eq)]
struct DiscoveryOutcome {
    keep_notify_discovery: bool,
    drain_pending: bool,
    bytes_read: usize,
}

/// The next deadline `interval` after `now`, saturating to the farthest representable future instant.
///
/// `reconcile_interval_secs` takes any `u64`, and a value large enough to mean "effectively never"
/// overflows the clock -- which must not panic the source. Reusing an already-expired deadline would
/// turn that setting into a busy reconciliation loop, so find the largest delay this clock accepts.
fn schedule_after(now: time::Instant, interval: Duration, current: time::Instant) -> time::Instant {
    let Some(deadline) = now.checked_add(interval) else {
        let mut fallback = Duration::MAX;
        while now.checked_add(fallback).is_none() {
            fallback /= 2;
        }
        return now
            .checked_add(fallback)
            .expect("a halved duration must eventually fit in an Instant")
            .max(current);
    };
    deadline
}

/// `FileServer` is a Source which cooperatively schedules reads over files,
/// converting the lines of said files into `LogLine` structures.
///
/// `FileServer` is configured on a path to watch. The files do _not_ need to
/// exist at startup.
///
/// By default (see [`FileDiscoveryMode::PollingOnly`]) `FileServer` discovers changes by polling:
/// it re-globs its configured paths on a fixed interval (`glob_minimum_cooldown`), so new files
/// are discovered in at most that interval. This works identically across every OS with POSIX-ish
/// filesystem semantics, at the cost of holding an open file handle for, and periodically
/// re-fingerprinting, every matched file regardless of activity.
///
/// Optionally (see [`FileDiscoveryMode::Notify`]), `FileServer` can instead use OS-level file
/// system event notifications (inotify/FSEvents/`ReadDirectoryChangesW`, via the `notify` crate)
/// to discover changes promptly, without needing to poll. A much less frequent polling pass
/// (`reconcile_interval`) still runs as a correctness backstop. Discovering files quickly doesn't
/// by itself stop them from holding an open handle once discovered; that's controlled separately
/// by `idle_timeout`, which applies regardless of discovery mode.
pub struct FileServer<PP, E: FileSourceInternalEvents>
where
    PP: PathsProvider,
{
    pub paths_provider: PP,
    pub max_read_bytes: usize,
    pub ignore_checkpoints: bool,
    pub read_from: ReadFrom,
    pub ignore_before: Option<DateTime<Utc>>,
    pub max_line_bytes: usize,
    pub line_delimiter: Bytes,
    pub data_dir: PathBuf,
    pub glob_minimum_cooldown: Duration,
    pub fingerprinter: Fingerprinter,
    pub oldest_first: bool,
    pub remove_after: Option<Duration>,
    pub emitter: E,
    pub rotate_wait: Duration,
    /// Controls whether `FileServer` uses OS-level file system event notifications (via the
    /// `notify` crate) to drive discovery/read-wakeups, in addition to the periodic glob
    /// rescan. See [`FileDiscoveryMode`] for details. `FileDiscoveryMode` itself implements
    /// [`Default`] (yielding [`FileDiscoveryMode::PollingOnly`]), so `FileDiscoveryMode::default()`
    /// can be used by callers who don't want to opt into notify-based discovery.
    pub discovery_mode: FileDiscoveryMode,
    /// How often to run the full glob+fingerprint reconciliation pass when
    /// [`FileDiscoveryMode::Notify`] is in use. This exists purely as a correctness backstop
    /// (OS notification queues can silently overflow, and there's a startup TOCTOU window
    /// before the watch is established) so it can be much less frequent than
    /// `glob_minimum_cooldown` was under the old polling-only model. Ignored when
    /// `discovery_mode` is `PollingOnly`, in which case `glob_minimum_cooldown` is used as
    /// before.
    pub reconcile_interval: Duration,
    /// How long an actively-open, EOF'd file must go without new writes before its file handle
    /// is closed and it is moved to the passive "Idle" watching state (still checkpointed, still
    /// polled for new data via cheap `fs::metadata` stats, but no open file descriptor). `None`
    /// disables this behavior entirely, i.e. files are never deactivated -- restoring the
    /// pre-existing, always-open behavior -- both at runtime (the `deactivate()` transition
    /// gated on this field directly) and at startup (`FileWatcher::new`'s fast path for
    /// `ignore_older`-excluded files, gated via `idle_on_startup = self.idle_timeout.is_some()`,
    /// since that path is a separate mechanism from `deactivate()` and would otherwise still
    /// start such files `Idle` regardless of this setting). Applies under both
    /// `FileDiscoveryMode::PollingOnly` and `FileDiscoveryMode::Notify`: notify-based discovery
    /// makes finding files fast, but doesn't by itself stop already-discovered,
    /// `ignore_older`-excluded files from holding a handle open for as long as they exist on
    /// disk -- this option is what does that, addressing the other half of
    /// <https://github.com/vectordotdev/vector/issues/3567>.
    pub idle_timeout: Option<Duration>,
}

/// Controls how `FileServer` discovers new files, renames, and wakes up reads of existing
/// files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileDiscoveryMode {
    /// The original behavior: re-glob and re-fingerprint every matched file every
    /// `glob_minimum_cooldown`. Simple, and works identically on every platform, but expensive
    /// when a very large number of files match the `include` patterns (see
    /// <https://github.com/vectordotdev/vector/issues/3567>), since every matched file is kept
    /// open and re-fingerprinted every cycle regardless of activity.
    #[default]
    PollingOnly,
    /// Event-driven discovery: watch the directories implied by the `include` patterns for
    /// OS-level create/modify/rename/remove notifications (via the `notify` crate) and use
    /// those to trigger discovery/read-wakeups promptly, instead of waiting for the next fixed
    /// poll interval. A full glob+fingerprint reconciliation pass still runs on
    /// `reconcile_interval` as a correctness backstop for dropped/overflowed OS events and the
    /// startup TOCTOU window. If the underlying OS watcher fails to initialize (e.g. platform
    /// resource limits), `FileServer` logs a warning and transparently falls back to
    /// polling-only behavior using `reconcile_interval` as the poll interval.
    Notify,
}

/// `FileServer` as Source
///
/// The 'run' of `FileServer` performs the cooperative scheduling of reads over
/// `FileServer`'s configured files. Much care has been taking to make this
/// scheduling 'fair', meaning busy files do not drown out quiet files or vice
/// versa but there's no one perfect approach. Very fast files _will_ be lost if
/// your system aggressively rolls log files. `FileServer` will keep a file
/// handler open but should your system move so quickly that a file disappears
/// before `FileServer` is able to open it the contents will be lost. This should be a
/// rare occurrence.
///
/// Specific operating systems support evented interfaces that correct this problem; see
/// [`FileDiscoveryMode::Notify`] for `FileServer`'s (opt-in) use of one via the `notify` crate.
impl<PP, E> FileServer<PP, E>
where
    PP: PathsProvider,
    E: FileSourceInternalEvents,
{
    // The first `shutdown_data` signal here is to stop this file
    // server from outputting new data; the second
    // `shutdown_checkpointer` is for finishing the background
    // checkpoint writer task, which has to wait for all
    // acknowledgements to be completed.
    pub async fn run<C, S1, S2>(
        mut self,
        mut chans: C,
        mut shutdown_data: S1,
        shutdown_checkpointer: S2,
        mut checkpointer: Checkpointer,
    ) -> Result<Shutdown, <C as Sink<Vec<Line>>>::Error>
    where
        C: Sink<Vec<Line>> + Unpin,
        <C as Sink<Vec<Line>>>::Error: std::error::Error,
        S1: Future + Unpin + Send + 'static,
        S2: Future + Unpin + Send + 'static,
    {
        let mut fp_map: IndexMap<FileFingerprint, FileWatcher> = Default::default();

        let mut backoff_cap: usize = 1;
        let mut lines = Vec::new();

        checkpointer.read_checkpoints(self.ignore_before).await;

        let mut known_small_files = file_source_common::KnownSmallFiles::default();

        // If we're using notify-driven discovery, establish the OS-level watch(es) *before*
        // doing the initial glob scan below. This closes (or at least drastically narrows) the
        // classic TOCTOU gap where a file changes between an initial scan and when the watch is
        // actually established: any change that lands in that window will still generate a
        // notify event, which will be sitting in the channel by the time the main loop starts
        // selecting on it, and will trigger a reconciliation pass that picks it up. The
        // alternative order (scan first, then watch) has a real gap in which changes are simply
        // lost until the next backstop reconciliation interval.
        //
        // If notify initialization fails (e.g. platform resource limits like hitting the
        // inotify instance cap), we log and transparently fall back to polling-only behavior
        // using the normal `glob_minimum_cooldown` cadence, rather than failing the whole file
        // source.
        let include_patterns = self.paths_provider.watch_roots();
        let mut notify_discovery = match self.discovery_mode {
            FileDiscoveryMode::Notify if !include_patterns.is_empty() => {
                match NotifyDiscovery::new(&include_patterns, &self.emitter).await {
                    Ok(discovery) => Some(discovery),
                    Err(error) => {
                        warn!(
                            message = "Failed to initialize OS-level file watcher; falling back to periodic polling.",
                            %error,
                        );
                        self.emitter
                            .emit_file_watch_backend_error(&std::io::Error::other(
                                error.to_string(),
                            ));
                        None
                    }
                }
            }
            FileDiscoveryMode::Notify => {
                warn!(
                    message = "Notify-based discovery requested but the configured paths provider does not expose watch roots; falling back to periodic polling.",
                );
                None
            }
            FileDiscoveryMode::PollingOnly => None,
        };

        let mut existing_files = Vec::new();
        for path in self.paths_provider.paths().into_iter() {
            if let Some(file_id) = self
                .fingerprinter
                .fingerprint_or_emit(&path, &mut known_small_files, &self.emitter)
                .await
            {
                existing_files.push((path, file_id));
            }
        }

        let metadata = join_all(
            existing_files
                .iter()
                .map(|(path, _file_id)| fs::metadata(path)),
        )
        .await;

        let created = metadata.into_iter().map(|m| {
            m.and_then(|m| m.created())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now())
        });

        let mut existing_files: Vec<(DateTime<Utc>, PathBuf, FileFingerprint)> = existing_files
            .into_iter()
            .zip(created)
            .map(|((path, file_id), key)| (key, path, file_id))
            .collect();

        existing_files.sort_by_key(|(key, _, _)| *key);

        let checkpoints = checkpointer.view();

        for (_key, path, file_id) in existing_files {
            self.watch_new_file(path, file_id, &mut fp_map, &checkpoints, true)
                .await;
        }
        self.emit_open_and_idle_counts(&fp_map);

        let mut stats = TimingStats::default();

        // Spawn the checkpoint writer task
        let checkpoint_task_handle = vector_common::spawn_in_current_span(checkpoint_writer(
            checkpointer,
            CHECKPOINT_WRITE_INTERVAL,
            shutdown_checkpointer,
            self.emitter.clone(),
        ));

        // Sleeping after reads keeps CPU down; `backoff_cap` grows exponentially on empty reads up to
        // a fixed limit. Re-globbing and checkpoint writes are deliberately not every iteration.
        //
        // `PollingOnly` re-scans on `glob_minimum_cooldown`. `Notify` re-scans on an OS event or on
        // the much longer `reconcile_interval` backstop (for queue overflow, changes before the watch
        // was established, and paths notify cannot watch). An event runs the *same* reconciliation
        // rather than interpreting notify's payload incrementally, which would risk diverging from
        // logic that is already correct.
        let mut next_glob_time = time::Instant::now();
        // The very first loop iteration always runs a discovery pass regardless of discovery
        // mode: `next_glob_time` was just set to `Instant::now()` above, and `now_time` inside the
        // loop is captured strictly later, so `next_glob_time <= now_time` is unconditionally true
        // on that first check -- no separate "force the first pass" flag is needed. This pass must
        // not be treated as notify-triggered (that would wrongly nudge every watcher's read pacing
        // via an `All`/`Paths` wakeup on startup, and under `PollingOnly` a notify-triggered
        // pass should never happen at all), so `pending_notify_wakeup` starts at `None`.
        let mut pending_notify_wakeup = NotifyWakeup::default();
        // Throttles notify-triggered full reconciliation passes independently of the backstop
        // timer (`next_glob_time`/`discovery_interval`): see `MIN_NOTIFY_DISCOVERY_INTERVAL`'s
        // doc comment for why. Starts at "now" so the very first notify event, whenever it
        // arrives, is handled immediately rather than waiting out this interval from process
        // start for no reason.
        let mut next_notify_discovery_time = time::Instant::now();
        // Keep this separate from the wakeup itself. A truncated candidate set gets a bounded
        // recovery window after the wakeup is consumed; otherwise the next main-loop iteration
        // could reap a watcher before the next reconciliation pass has had a chance to find the
        // omitted rename destination. The deadline is never extended by an ordinary wakeup or by
        // an unrelated watcher that happens to remain unfindable.
        let mut rename_recovery_deadline = None;
        let mut previous_discovery_interval = self.glob_minimum_cooldown;
        loop {
            // A notify watcher with complete coverage can use the long reconciliation backstop.
            // If a root registration failed, keep retrying the full glob pass on the ordinary
            // polling cadence: no event can arrive from that root, so waiting for the long notify
            // backstop can miss a file that is created and removed in between. The same fallback
            // applies while notify is absent or being torn down.
            let notify_covers_everything = notify_discovery
                .as_ref()
                .is_some_and(|discovery| !discovery.has_uncovered_roots());
            let discovery_interval =
                if self.discovery_mode == FileDiscoveryMode::Notify && notify_covers_everything {
                    self.reconcile_interval
                } else {
                    self.glob_minimum_cooldown
                };

            // Glob find files to follow, but not too often. A pending notify wakeup only
            // triggers this early (ahead of `next_glob_time`) once `next_notify_discovery_time`
            // has also elapsed -- see `MIN_NOTIFY_DISCOVERY_INTERVAL`.
            let now_time = time::Instant::now();
            // Raise this as soon as the truncated wakeup is pending, not only after a
            // reconciliation consumes it. Notify throttling can leave a wakeup queued for a few
            // iterations, and the reaping check below must be conservative during that window.
            rename_recovery_deadline = update_rename_recovery_deadline(
                rename_recovery_deadline,
                pending_notify_wakeup.rename_paths_incomplete(),
                now_time,
                discovery_interval,
            );
            let notify_wakeup_ready =
                pending_notify_wakeup.is_pending() && next_notify_discovery_time <= now_time;
            if discovery_interval < previous_discovery_interval {
                // Coverage can become incomplete during the discovery pass that scheduled the
                // current (long) deadline. Pull it in immediately rather than waiting until the
                // old `reconcile_interval` expires before the fallback polling pass runs.
                let fallback_deadline =
                    schedule_after(now_time, discovery_interval, next_glob_time);
                if fallback_deadline < next_glob_time {
                    next_glob_time = fallback_deadline;
                }
            }
            previous_discovery_interval = discovery_interval;
            // Idle watchers have no open handle and therefore do not get visited by the normal
            // read loop. Do their deadline-only cleanup independently of reconciliation, but let a
            // due discovery pass inspect notify-named files first so a just-arrived write can
            // reactivate an idle watcher instead of being removed at the same instant.
            if self.idle_removal_due(&fp_map)
                && next_glob_time > now_time
                && !pending_notify_wakeup.is_pending()
            {
                self.remove_idle_watchers_due(&mut fp_map, &mut lines).await;
                // `remove_idle_watchers_due` can reactivate a file after its metadata changed.
                // Reuse the same checkpoint handoff as the regular idle-poll path: a replacement
                // inode or truncation must not read new content under the old generation/offset.
                for (&file_id, watcher) in &mut fp_map {
                    if watcher.take_reader_restarted() {
                        checkpoints.register(file_id, 0, watcher.take_new_generation());
                    }
                }
            }
            let mut discovery_bytes_read = 0;
            if next_glob_time <= now_time || notify_wakeup_ready {
                // Leave the wakeup queued (don't take it) if we're here only because the backstop
                // timer fired while the notify throttle hasn't elapsed yet.
                let backstop_due = next_glob_time <= now_time;
                let woken_by_notify_event = if notify_wakeup_ready {
                    next_notify_discovery_time = schedule_after(
                        now_time,
                        MIN_NOTIFY_DISCOVERY_INTERVAL,
                        next_notify_discovery_time,
                    );
                    pending_notify_wakeup.take()
                } else {
                    NotifyWakeup::default()
                };
                // Only when the backstop itself came due. Pushing it out on every notify pass lets a
                // file that is written to continuously postpone it indefinitely -- and it is the
                // backstop that recovers a creation whose event the backend dropped.
                if backstop_due {
                    next_glob_time = schedule_after(now_time, discovery_interval, next_glob_time);
                }

                if stats.started_at.elapsed() > Duration::from_secs(1) {
                    stats.report();
                }

                if stats.started_at.elapsed() > Duration::from_secs(10) {
                    stats = TimingStats::default();
                }

                let start = time::Instant::now();
                let discovery_outcome = self
                    .discover(
                        &mut fp_map,
                        &mut known_small_files,
                        &checkpoints,
                        notify_discovery.as_mut(),
                        &woken_by_notify_event,
                        &mut lines,
                    )
                    .await;
                discovery_bytes_read = discovery_outcome.bytes_read;
                if discovery_outcome.drain_pending {
                    // A rotation drain is deliberately a continuation of the normal read loop,
                    // not an unbounded discovery-side operation. Retry the reconciliation as soon
                    // as this bounded batch has been handed downstream so the replacement can be
                    // opened without waiting for the ordinary polling interval.
                    next_glob_time = time::Instant::now();
                }
                if !discovery_outcome.keep_notify_discovery {
                    warn!(
                        "Notify-based discovery unavailable; relying on periodic reconciliation only."
                    );
                    // `NotifyDiscovery::drop` hands the watcher to a detached teardown thread, so
                    // this never runs `notify`'s own (possibly blocking/panicking) `Drop` here.
                    notify_discovery = None;
                }
                stats.record("discovery", start.elapsed());

                let start = time::Instant::now();
                // An event can be pending during the notify throttle while this pass is being
                // run for the ordinary reconciliation timer. Idle rename recovery may use those
                // named paths immediately; unlike active-read nudges, it does not bypass any
                // read pacing and is needed to avoid reaping a cross-directory rotation first.
                let idle_poll_wakeup = if woken_by_notify_event.is_pending() {
                    &woken_by_notify_event
                } else {
                    &pending_notify_wakeup
                };
                self.poll_idle_watchers(&mut fp_map, &mut lines, idle_poll_wakeup)
                    .await;
                // `reactivate` rewinds to zero when the file it reopens turns out to be a
                // replacement or to have been truncated, so the persisted offset belongs to content
                // that is gone. Swept here for the same reason the discovery passes sweep: a restart
                // before the first new line is acknowledged would otherwise resume past its prefix.
                for (&file_id, watcher) in &mut fp_map {
                    if watcher.take_reader_restarted() {
                        checkpoints.register(file_id, 0, watcher.take_new_generation());
                    }
                }
                stats.record("idle-poll", start.elapsed());
            }

            // Cleanup the known_small_files
            if let Some(grace_period) = self.remove_after {
                let mut set = JoinSet::new();

                // Entries are keyed by canonical identity but unlinked by the path the
                // configuration named: removing a symlink's canonical target would delete a file
                // outside the watched path. The identity travels with the task so the entry can be
                // dropped once the unlink succeeds.
                let remove_file_tasks: HashMap<Id, PathBuf> = known_small_files
                    .expired(grace_period)
                    .into_iter()
                    .map(|(identity, removal_path)| {
                        let task_path = removal_path.clone();
                        let abort_handle =
                            set.spawn(async move { (identity, remove_file(&task_path).await) });
                        (abort_handle.id(), removal_path)
                    })
                    .collect();

                while let Some(res) = set.join_next().await {
                    match res {
                        // The task reports the map *key* (a canonical identity); the event carries
                        // the path that was actually unlinked, which is what the user configured.
                        Ok((identity, Ok(()))) => {
                            if let Some(removal_path) = known_small_files
                                .removal_path(&identity)
                                .map(Path::to_path_buf)
                            {
                                known_small_files.remove(&identity, &removal_path);
                                self.emitter.emit_file_deleted(&removal_path);
                            }
                        }
                        Ok((identity, Err(err))) => {
                            let path = known_small_files
                                .removal_path(&identity)
                                .map(Path::to_path_buf)
                                .unwrap_or_else(|| identity.clone());
                            // A gone spelling must not be chosen again (see `forget_missing_spelling`).
                            if err.kind() == std::io::ErrorKind::NotFound {
                                known_small_files.forget_missing_spelling(&identity, &path);
                            }
                            self.emitter.emit_file_delete_error(&path, err);
                        }
                        Err(join_err) => {
                            self.emitter.emit_file_delete_error(
                                remove_file_tasks
                                    .get(&join_err.id())
                                    .expect("panicked/cancelled task id not in task id pool"),
                                std::io::Error::other(join_err),
                            );
                        }
                    }
                }
            }

            // Collect lines by polling files.
            let mut global_bytes_read: usize = discovery_bytes_read;
            let mut maxed_out_reading_single_file = false;
            for (&file_id, watcher) in &mut fp_map {
                if !watcher.should_read() {
                    continue;
                }

                let start = time::Instant::now();
                let mut bytes_read: usize = 0;
                while let Ok(RawLineResult {
                    raw_line: Some(line),
                    discarded_for_size_and_truncated,
                }) = watcher.read_line().await
                {
                    discarded_for_size_and_truncated.iter().for_each(|buf| {
                        self.emitter.emit_file_line_too_long(
                            &buf.clone(),
                            self.max_line_bytes,
                            buf.len(),
                        )
                    });

                    let sz = line.bytes.len();
                    trace!(
                        message = "Read bytes.",
                        path = ?watcher.path,
                        bytes = ?sz
                    );
                    stats.record_bytes(sz);

                    bytes_read += sz;

                    lines.push(Line {
                        text: line.bytes,
                        filename: watcher.path.to_str().expect("not a valid path").to_owned(),
                        file_id,
                        generation: watcher.generation(),
                        start_offset: line.offset,
                        end_offset: watcher.get_file_position(),
                    });

                    if bytes_read > self.max_read_bytes {
                        maxed_out_reading_single_file = true;
                        break;
                    }
                }
                stats.record("reading", start.elapsed());

                if bytes_read > 0 {
                    global_bytes_read = global_bytes_read.saturating_add(bytes_read);
                } else {
                    // Should the file be removed
                    if let Some(grace_period) = self.remove_after
                        && watcher.last_read_success().elapsed() >= grace_period
                    {
                        if !watcher.removal_is_authorized() {
                            salvage_final_partial_line(watcher, file_id, &mut lines);
                            watcher.set_dead();
                        } else {
                            // Try to remove
                            match remove_file(&watcher.path).await {
                                Ok(()) => {
                                    self.emitter.emit_file_deleted(&watcher.path);
                                    salvage_final_partial_line(watcher, file_id, &mut lines);
                                    watcher.set_dead();
                                }
                                Err(error) => {
                                    // We will try again after some time.
                                    self.emitter.emit_file_delete_error(&watcher.path, error);
                                }
                            }
                        }
                    }

                    // Quiet past `idle_timeout` after EOF: close the handle and move to `Idle`,
                    // keeping the checkpoint and polling cheaply via `fs::metadata` instead.
                    // Runtime half of the #3567 fix; `reactivate` (see `SkipPrefixReader`)
                    // handles resuming gzip files correctly too.
                    if !watcher.dead()
                        && let Some(idle_timeout) = self.idle_timeout
                        && watcher.reached_eof()
                        && watcher.idle_for().is_some_and(|idle| idle >= idle_timeout)
                    {
                        watcher.deactivate().await;
                    }
                }

                // Do not move on to newer files if we are behind on an older file
                if self.oldest_first && maxed_out_reading_single_file {
                    break;
                }
            }

            for (&file_id, watcher) in &mut fp_map {
                if watcher.file_findable() {
                    continue;
                }
                // See `should_reap_unfindable_watcher`'s doc comment for why `Idle` and `Active`
                // watchers need different grace periods here.
                if should_reap_unfindable_watcher(
                    watcher.is_idle(),
                    watcher.path_is_outside_glob(),
                    watcher.unfindable_for(),
                    discovery_interval,
                    self.rotate_wait,
                    rename_recovery_deadline
                        .is_some_and(|deadline| deadline > time::Instant::now()),
                ) {
                    salvage_final_partial_line(watcher, file_id, &mut lines);
                    watcher.set_dead();
                }
            }

            // A FileWatcher is dead when the underlying file has disappeared.
            // If the FileWatcher is dead we don't retain it; it will be deallocated.
            fp_map.retain(|file_id, watcher| {
                if watcher.dead() {
                    self.emitter
                        .emit_file_unwatched(&watcher.path, watcher.reached_eof());
                    checkpoints.set_dead(*file_id, watcher.generation());
                    false
                } else {
                    true
                }
            });
            self.emit_open_and_idle_counts(&fp_map);

            let start = time::Instant::now();
            let to_send = std::mem::take(&mut lines);

            let result = chans.send(to_send).await;
            match result {
                Ok(()) => {}
                Err(error) => {
                    error!(message = "Output channel closed.", %error);
                    return Err(error);
                }
            }
            stats.record("sending", start.elapsed());

            let start = time::Instant::now();
            // When no lines have been read we kick the backup_cap up by twice,
            // limited by the hard-coded cap. Else, we set the backup_cap to its
            // minimum on the assumption that next time through there will be
            // more lines to read promptly.
            backoff_cap = if global_bytes_read == 0 {
                cmp::min(2_048, backoff_cap.saturating_mul(2))
            } else {
                1
            };
            let backoff = backoff_cap.saturating_sub(global_bytes_read);

            // This works only if run inside tokio context since we are using
            // tokio's Timer. Outside of such context, this will panic on the first
            // call. Also since we are using block_on here and in the above code,
            // this should be run in its own thread. `spawn_blocking` fulfills
            // all of these requirements.

            // Capped at `next_notify_discovery_time` when a notify wakeup is already pending:
            // otherwise, once `backoff_cap` has grown large from a quiet spell, a pending wakeup
            // with no further events to cut the sleep short (see the `tokio::select!` below) would
            // wait out the full backoff instead of the much shorter `MIN_NOTIFY_DISCOVERY_INTERVAL`
            // throttle it's actually waiting on. Uses a fresh `Instant::now()`, not the `now_time`
            // captured at the top of the loop: `discover`/reading files/sending downstream can
            // take a while, and computing the remaining time against a stale timestamp would
            // overstate it, adding back some of the latency this cap exists to remove.

            let mut sleep_duration = if pending_notify_wakeup.is_pending() {
                Duration::from_millis(backoff as u64)
                    .min(next_notify_discovery_time.saturating_duration_since(time::Instant::now()))
            } else {
                Duration::from_millis(backoff as u64)
            };
            // A notify-only source may otherwise sleep until its much later reconciliation
            // backstop after an idle watcher becomes eligible for removal. Capping the existing
            // backoff sleep is enough to wake the loop; the deadline-only pass above performs the
            // removal without stat-ing every idle file.
            if !pending_notify_wakeup.is_pending()
                && let Some(idle_removal_delay) = self.next_idle_removal_delay(&fp_map)
            {
                sleep_duration = sleep_duration.min(idle_removal_delay);
            }
            let sleep_fut = async move {
                if !sleep_duration.is_zero() {
                    sleep(sleep_duration).await;
                }
            };
            futures::pin_mut!(sleep_fut);

            // When notify-based discovery is active, race the backoff sleep against both
            // shutdown and the notify event channel, so a filesystem event can cut the sleep
            // short and trigger a prompt reconciliation pass instead of waiting out the (small,
            // but nonzero) backoff. `shutdown_data: S1: Future + Unpin` so it's safe to poll by
            // mutable reference across loop iterations without re-pinning.
            if let Some(discovery) = notify_discovery.as_mut() {
                let mut shutdown = false;
                // Set when notify-based discovery must be disabled: either the channel closed
                // (the watcher task/thread went away, e.g. panicked) or a backend error left the
                // watcher unrebuildable. Either way we fall back to periodic reconciliation only,
                // not treating it as fatal to the file source.
                let mut disable_notify = false;
                tokio::select! {
                    biased;
                    _ = &mut shutdown_data => {
                        shutdown = true;
                    }
                    msg = discovery.recv() => {
                        let channel_closed = msg.is_none();
                        disable_notify = channel_closed
                            || !self
                                .handle_notify_message(msg, discovery, &mut pending_notify_wakeup)
                                .await;
                        // Briefly drain/debounce further events so a burst of writes collapses
                        // into a single reconciliation pass. Each drained message still goes
                        // through the same handling as the message above (not just discarded):
                        // a `BackendError`/`Overflow` arriving inside this window must still
                        // trigger `forget_watches`/overflow telemetry, or those effects would be
                        // silently dropped whenever they happen to land within
                        // `NOTIFY_EVENT_DEBOUNCE` of another event, which -- for a backend error
                        // specifically -- would leave `forget_watches` never called and the lost
                        // watch registration never re-established.
                        if !disable_notify {
                            let drain_result = tokio::time::timeout(NOTIFY_EVENT_DEBOUNCE, async {
                                loop {
                                    let msg = discovery.recv().await;
                                    let is_none = msg.is_none();
                                    if is_none
                                        || !self
                                            .handle_notify_message(
                                                msg,
                                                discovery,
                                                &mut pending_notify_wakeup,
                                            )
                                            .await
                                    {
                                        return true;
                                    }
                                }
                            })
                            .await;
                            // A timeout just means the debounce window elapsed while events were
                            // still arriving, which is the expected/common case; the drain loop
                            // otherwise returns `true` on either a closed channel or a failed
                            // watcher rebuild.
                            disable_notify = drain_result.unwrap_or(false);
                        }
                        pending_notify_wakeup.retry_canonical_paths().await;
                    }
                    _ = &mut sleep_fut => {}
                }
                if disable_notify {
                    warn!(
                        "Notify-based discovery unavailable; relying on periodic reconciliation only."
                    );
                    notify_discovery = None;
                }
                stats.record("sleeping", start.elapsed());
                if shutdown {
                    chans
                        .close()
                        .await
                        .expect("error closing file_server data channel.");
                    let checkpointer = checkpoint_task_handle
                        .await
                        .expect("checkpoint task has panicked");
                    if let Err(error) = checkpointer.write_checkpoints().await {
                        error!(?error, "Error writing checkpoints before shutdown");
                    }
                    return Ok(Shutdown);
                }
                continue;
            }

            match select(shutdown_data, sleep_fut).await {
                Either::Left((_shutdown_token, _)) => {
                    chans
                        .close()
                        .await
                        .expect("error closing file_server data channel.");
                    let checkpointer = checkpoint_task_handle
                        .await
                        .expect("checkpoint task has panicked");
                    if let Err(error) = checkpointer.write_checkpoints().await {
                        error!(?error, "Error writing checkpoints before shutdown");
                    }
                    return Ok(Shutdown);
                    // _shutdown_token is dropped here, after checkpoints are written,
                    // which signals shutdown_done to the caller.
                }
                Either::Right((_, future)) => shutdown_data = future,
            }
            stats.record("sleeping", start.elapsed());
        }
    }

    /// Handle a single message received from `discovery`, both for the initial message that woke
    /// up the `tokio::select!` in `run` and for each message drained from the channel during the
    /// subsequent debounce window. `None` (the channel having closed) is intentionally not
    /// matched here: the caller is responsible for detecting that (it needs to stop the drain
    /// loop and fall back off notify entirely), whereas every other variant is handled
    /// identically regardless of whether it arrived as the "woke us up" message or as one drained
    /// during debounce -- in particular, a `BackendError`'s `forget_watches()` call and an
    /// `Overflow`'s telemetry must fire even when they land inside the debounce window, not just
    /// on the message that started it.
    /// Returns `false` if notify-based discovery must be disabled entirely (the watcher failed to
    /// rebuild after a backend error), `true` otherwise.
    #[must_use]
    async fn handle_notify_message(
        &self,
        msg: Option<NotifyMessage>,
        discovery: &mut NotifyDiscovery,
        pending_notify_wakeup: &mut NotifyWakeup,
    ) -> bool {
        match msg {
            Some(NotifyMessage::PathsChanged(paths)) => {
                trace!(message = "Received file change notification.", ?paths);
                // Named paths only: `discover`'s per-watcher nudge (`FileWatcher::mark_ready_to_read`)
                // should only touch watchers this event actually concerns, not every tracked file --
                // see `NotifyWakeup`'s docs for why nudging everything on every event doesn't scale.
                pending_notify_wakeup.add_paths(paths);
            }
            Some(NotifyMessage::PathsCreated(paths)) => {
                trace!(message = "Received file creation notification.", ?paths);
                pending_notify_wakeup.add_created_paths(paths);
            }
            Some(NotifyMessage::PathsRemoved(paths)) => {
                trace!(message = "Received file removal notification.", ?paths);
                // A removal or rename of a registered root can leave the backend watch attached
                // to the old inode (Linux/inotify keeps it attached after a directory rename).
                // Rebuild the whole watcher immediately so the old registration is detached
                // before a recreated path is installed; bookkeeping-only invalidation would keep
                // accumulating watches on renamed roots.
                let teardown_failed = paths.iter().any(|path| discovery.is_watched_dir(path))
                    && !discovery.forget_watches();
                // Record the paths *before* reporting the failure: returning early dropped them, so
                // the caller disabled notify with no pending wakeup and waited out
                // `reconcile_interval` (300s by default) -- long enough to miss a recreated or
                // rotated file entirely. A coarse wakeup is the right shape here, since a failed
                // rebuild means the registrations are gone and nothing else will name those paths.
                if teardown_failed {
                    pending_notify_wakeup.mark_all();
                    return false;
                }
                pending_notify_wakeup.add_removed_paths(paths);
            }
            Some(NotifyMessage::Overflow) => {
                self.emitter.emit_file_watch_events_overflowed();
                // No specific paths are known to have changed; treat every tracked watcher as
                // possibly needing a nudge, same as the pre-existing coarse behavior.
                pending_notify_wakeup.mark_all();
                // An overflow means some events were dropped -- possibly including a
                // `PathsRemoved` for a watched directory, which (e.g. on Linux/inotify) can
                // invalidate the OS-level watch on that inode. Since we can't tell which events
                // were lost, rebuild the watcher from scratch, same as on a `BackendError` below.
                if !discovery.forget_watches() {
                    return false;
                }
            }
            Some(NotifyMessage::BackendError(error)) => {
                self.emitter
                    .emit_file_watch_backend_error(&std::io::Error::other(error));
                // A backend error can mean the watcher silently dropped a watch (e.g. a watched
                // directory was removed and recreated). Rebuild the watcher and its bookkeeping
                // from scratch so the upcoming reconciliation pass's `resync_watches` call
                // re-`watch`es everything, rather than trusting stale registrations. See
                // `NotifyDiscovery::forget_watches` for why a full rebuild is necessary here.
                if !discovery.forget_watches() {
                    return false;
                }
                // No specific paths are known to have changed here either.
                pending_notify_wakeup.mark_all();
            }
            None => {}
        }
        pending_notify_wakeup.resolve_canonical_paths().await;
        true
    }

    /// Perform a full glob+fingerprint reconciliation pass: re-glob the configured `include`
    /// patterns and detect new files, renames (a known fingerprint appearing at a new path), and
    /// duplicate-fingerprint conflicts (picking the most recently modified file).
    ///
    /// Runs on a fixed interval (`glob_minimum_cooldown`, or `reconcile_interval` as the `Notify`
    /// backstop) and for events that may create, remove, or rename. A plain modification takes the
    /// targeted path instead, so sustained writes to one file do not rescan every matched file.
    ///
    /// `notify_wakeup` says whether a real filesystem event triggered this pass. Only a watcher it
    /// names -- or an `All` wakeup -- gets nudged past its read-pacing timers: the periodic timer
    /// alone is no evidence that a given file changed, and nudging every watcher regardless cost an
    /// O(N) sweep per event under a large `include`. Matching also compares canonical paths, so a
    /// symlink alias keeps the low-latency nudge; canonicalization happens once per event path.
    /// Returns whether notify-based discovery should remain enabled, whether a bounded rotation
    /// drain needs another pass, and how many bytes the drain emitted.
    #[must_use]
    async fn discover(
        &mut self,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        known_small_files: &mut file_source_common::KnownSmallFiles,
        checkpoints: &CheckpointsView,
        notify_discovery: Option<&mut NotifyDiscovery>,
        notify_wakeup: &NotifyWakeup,
        lines: &mut Vec<Line>,
    ) -> DiscoveryOutcome {
        // Defensive resync: cheap to call, and covers the (rare) case where the set of
        // directories implied by `include` patterns needs to change -- e.g. a literal include
        // path's directory didn't exist at startup and now does, or the `PathsProvider`
        // implementation's `watch_roots()` otherwise changes over time. New files created
        // *inside* an already-recursively-watched directory tree don't need this: the OS
        // backend (inotify/FSEvents/ReadDirectoryChangesW) follows new subdirectories on its
        // own once a recursive watch is established on their ancestor.
        let mut keep_notify_discovery = true;
        let mut full_scan_required = false;
        if let Some(discovery) = notify_discovery {
            keep_notify_discovery = discovery
                .resync_watches(&self.paths_provider.watch_roots(), &self.emitter)
                .await;
            // A sticky overflow/backend-error flag is consumed inside `resync_watches`, and it is
            // raised exactly when the channel was too full to carry even the coarse `Overflow`
            // substitute. The pending wakeup can therefore still be a narrow `Paths` set while an
            // unrelated creation event was dropped, so the targeted pass below must be skipped:
            // only the glob pass can find a file nothing named.
            // A failed resync leaves at least one configured root without an event source. Do a
            // full pass now while the existing watcher is still available, then let the main loop
            // fall back to periodic reconciliation rather than waiting for a notify event that can
            // never arrive from that root.
            let resync_full_scan_required = discovery.take_full_scan_required();
            full_scan_required = !keep_notify_discovery || resync_full_scan_required;
        }

        // A plain modification event names the only paths that need to be fingerprinted. Create,
        // remove, and rename events retain `rename_paths` and use the full pass below, as do the
        // periodic backstop and coarse overflow/backend-error wakeups.
        if let Some(paths) = notify_wakeup
            .targeted_change_paths()
            .filter(|_| !full_scan_required)
            && self
                .discover_changed_paths(paths, fp_map, known_small_files, checkpoints)
                .await
        {
            // Every named path belonged to a tracked file. Otherwise fall through to the full pass:
            // an unaccounted path may be a new file the `include` globs cover, and only the glob
            // pass can decide that -- waiting for the backstop would leave it unread.
            return DiscoveryOutcome {
                keep_notify_discovery,
                drain_pending: false,
                bytes_read: 0,
            };
        }

        for (_file_id, watcher) in &mut *fp_map {
            // Do not start the unfindable grace period until the full glob/fingerprint pass has
            // completed. A slow scan must not consume the watcher's grace period before absence
            // has actually been established.
            watcher.prepare_for_discovery();
        }

        // Computed once per pass (not once per file) and only when there's actually a pending
        // notify wakeup to compare against -- the common case, a backstop-timer-only pass with
        // `NotifyWakeup::None`, skips this (and every `.names()` call below) entirely, since
        // `None` never matches regardless of what `path` is compared against.
        let cwd_for_notify_comparison = notify_wakeup
            .is_pending()
            .then(|| std::env::current_dir().ok())
            .flatten();

        let mut tracked_path_index = TrackedPathIndex::default();
        let mut drain_attempted = HashSet::new();
        let mut drain_pending = false;
        let mut drain_bytes_read: usize = 0;
        for path in self.paths_provider.paths().into_iter() {
            let outcome = self
                .fingerprinter
                .fingerprint_or_emit_detailed(
                    &path,
                    known_small_files,
                    &self.emitter,
                    // Only a path some watcher already tracks can need the prefix: it is compared
                    // against the one a pending rewind was taken for.
                    if fp_map.is_empty() {
                        PrefixWanted::No
                    } else {
                        PrefixWanted::Yes
                    },
                )
                .await;
            let rewrite_suspected = outcome.is_incomplete();
            let path_absent = outcome.is_absent();
            if let Some(file_id) = outcome.fingerprint() {
                if let Some(watcher) = fp_map.get_mut(&file_id) {
                    // file fingerprint matches a watched file
                    let was_found_this_cycle = watcher.file_findable();
                    if watcher.path == path {
                        // A same-path replacement is invisible to the fingerprint key when the
                        // replacement repeats the old first line. Verify the candidate identity
                        // before declaring this watcher refreshed; otherwise `update_path` would
                        // drop the old descriptor and its unread tail. Use the same bounded drain
                        // as the changed-fingerprint branch, even though no rekey is needed.
                        let mut refreshed = true;
                        let mut path_changed = false;
                        if !watcher.candidate_has_tracked_identity(path.clone()).await {
                            if drain_attempted.insert(file_id) {
                                match drain_and_repoint(
                                    watcher,
                                    file_id,
                                    path.clone(),
                                    lines,
                                    self.max_read_bytes,
                                )
                                .await
                                {
                                    Ok(DrainOutcome::Repointed { bytes_read, .. }) => {
                                        drain_bytes_read =
                                            drain_bytes_read.saturating_add(bytes_read);
                                        path_changed = true;
                                    }
                                    Ok(DrainOutcome::LimitReached { bytes_read }) => {
                                        drain_bytes_read =
                                            drain_bytes_read.saturating_add(bytes_read);
                                        drain_pending = true;
                                    }
                                    Err(error) => {
                                        self.emitter.emit_file_watch_error(&watcher.path, error);
                                        refreshed = false;
                                    }
                                }
                            }
                        } else if watcher.is_active() {
                            // A full reconciliation can be the first pass to observe an appended
                            // gzip member. Raise the raw-size baseline from the same stat so a later
                            // truncate to a size between the original and current members is not
                            // mistaken for ordinary growth.
                            let check = watcher.shrank_below_reader().await;
                            watcher.observe_raw_size(check.observed);
                        }
                        if path_changed {
                            tracked_path_index.paths_changed();
                        }
                        if refreshed {
                            watcher.set_file_findable(true);
                            // The file fingerprints again, so any rewrite it was mid-way through
                            // is over. This also releases the rewind guard after a same-fingerprint
                            // replacement has been repointed.
                            watcher.fingerprint_completed();
                        }
                        trace!(
                            message = "Continue watching file.",
                            path = ?path,
                        );
                        if watcher.is_idle() && !watcher.path_has_tracked_identity().await {
                            // A same-name replacement can retain both its size and mtime. Force
                            // the idle poll to reopen it so `reactivate` can compare identities
                            // instead of seeking to an offset belonging to the old inode.
                            watcher.invalidate_idle_bookkeeping();
                        }
                        let absolutized_path =
                            crate::absolutize(&path, cwd_for_notify_comparison.as_deref());
                        if notify_wakeup.names(&absolutized_path, watcher.canonical_path()) {
                            // A concrete filesystem event named this exact path (or we can't tell
                            // which paths changed, e.g. `Overflow`), so this watcher may have new
                            // data waiting even if it's currently mid-EOF-backoff or past the
                            // quiet-file throttle window (both of which exist only to pace
                            // *unprompted* polling, not to delay a read a real signal just
                            // justified). See `FileWatcher::mark_ready_to_read`.
                            watcher.mark_ready_to_read();
                        }
                    } else if !was_found_this_cycle {
                        // matches a file with a different path
                        info!(
                            message = "Watched file has been renamed.",
                            path = ?path,
                            old_path = ?watcher.path
                        );
                        // Keep the watcher unfindable until the new path is opened successfully.
                        // The path may disappear between fingerprinting and opening it.
                        if watcher.update_path(path).await.is_ok() {
                            watcher.set_file_findable(true);
                            watcher.fingerprint_completed();
                            tracked_path_index.paths_changed();
                        }
                    } else {
                        // This watcher was already matched by another path in this pass, so the
                        // original path remains valid even if switching to this newer duplicate
                        // fails.
                        watcher.set_file_findable(true);
                        info!(
                            message = "More than one file has the same fingerprint.",
                            path = ?path,
                            old_path = ?watcher.path
                        );
                        let (old_path, new_path) = (&watcher.path, &path);
                        if let (Ok(old_modified_time), Ok(new_modified_time)) = (
                            fs::metadata(old_path).await.and_then(|m| m.modified()),
                            fs::metadata(new_path).await.and_then(|m| m.modified()),
                        ) && old_modified_time < new_modified_time
                        {
                            info!(
                                message = "Switching to watch most recently modified file.",
                                new_modified_time = ?new_modified_time,
                                old_modified_time = ?old_modified_time,
                            );
                            // Failure is fine here: the next cycle retries. On success the file
                            // fingerprinted, so any rewrite it was mid-way through is over.
                            if watcher.update_path(path).await.is_ok() {
                                watcher.fingerprint_completed();
                                tracked_path_index.paths_changed();
                            }
                        }
                    }
                } else if let Some(stale_key) = {
                    // Resolved before the `if let` so no borrow of `fp_map` is live across the
                    // `await`: that would make `FileServer::run`'s future non-`Send`.
                    let canonical_candidate = fs::canonicalize(&path).await.ok();
                    tracked_path_index.key_for_path(
                        fp_map,
                        &path,
                        canonical_candidate.as_deref(),
                        cwd_for_notify_comparison.as_deref(),
                    )
                } {
                    // This path is already tracked, under a fingerprint it no longer has: an
                    // in-place rewrite changed the hashed prefix. Rekey rather than calling
                    // `watch_new_file`, which would add a *second* reader for one file and duplicate
                    // its output while the first one stays alive.
                    //
                    // Restart the reader first, exactly as the targeted pass does: the offset and
                    // buffered bytes describe content the rewrite discarded, so reading on would skip
                    // the new prefix or splice it onto the old tail. A failed reopen leaves the
                    // watcher under its old key so the next pass retries.
                    // Borrows are taken and released around each `await`: `FileWatcher` holds a
                    // `dyn AsyncBufRead` that is not `Sync`, so keeping even a shared reference
                    // across one makes `FileServer::run`'s future non-`Send`.
                    // The *candidate's* identity, not the watcher's own path: an alias that has since
                    // disappeared would answer `false` while this spelling still resolves to the same
                    // inode, and `update_path` would then preserve an offset into rewritten content.
                    let identity_check = fp_map
                        .get(&stale_key)
                        .map(|watcher| watcher.candidate_has_tracked_identity(path.clone()));
                    let same_inode = match identity_check {
                        Some(check) => check.await,
                        None => false,
                    };
                    let watcher_path = fp_map.get(&stale_key).map(|watcher| watcher.path.clone());

                    // Two different situations reach here, and both must reposition the reader
                    // before the watcher is filed under the new fingerprint:
                    //
                    // - Same inode: an in-place rewrite. `restart_after_rewrite` rewinds to zero.
                    // - Different inode: a same-name replacement. `update_path` reopens the new
                    //   file; rekeying without it would file the watcher under the replacement's
                    //   fingerprint while its reader stayed attached to the old inode, so the
                    //   replacement would never be read at all.
                    // The collision is checked *before* the reader is touched. `rekey_watcher`
                    // refuses a fingerprint another watcher already owns, and repositioning first
                    // would leave that reader rewound onto the rewritten content while still filed
                    // under the old key: it would emit those lines under the wrong identity, and the
                    // full pass could hand the same fingerprint to the other file, duplicating them.
                    if stale_key != file_id && fp_map.contains_key(&file_id) {
                        trace!(
                            message = "Fingerprint already owned by another watcher.",
                            path = ?path,
                        );
                        continue;
                    }
                    let mut restart_failed = false;
                    let mut drain_deferred = false;
                    let mut drained_checkpoint = None;
                    if let Some(tracked_path) = watcher_path {
                        // The replacement is reopened at the *discovered* path, not the watcher's own:
                        // with overlapping or symlinked includes the old alias can be gone while the
                        // canonical path is still yielded, and reopening the alias then fails and
                        // strands the watcher under a path that no longer exists.
                        let reopen_path = path.clone();
                        let reopened = match fp_map.get_mut(&stale_key) {
                            Some(watcher) => {
                                if !same_inode {
                                    // Finish the inode this reader still holds before it is pointed
                                    // at the one that replaced it. A path can be yielded more than
                                    // once through overlapping includes or aliases; do not spend a
                                    // second drain budget on the same watcher in one pass.
                                    if !drain_attempted.insert(stale_key) {
                                        drain_deferred = true;
                                        Ok(())
                                    } else {
                                        match drain_and_repoint(
                                            watcher,
                                            stale_key,
                                            reopen_path,
                                            lines,
                                            self.max_read_bytes,
                                        )
                                        .await
                                        {
                                            Ok(DrainOutcome::Repointed {
                                                bytes_read,
                                                old_position,
                                                old_generation,
                                            }) => {
                                                drain_bytes_read =
                                                    drain_bytes_read.saturating_add(bytes_read);
                                                drained_checkpoint =
                                                    Some((old_position, old_generation));
                                                Ok(())
                                            }
                                            Ok(DrainOutcome::LimitReached { bytes_read }) => {
                                                drain_bytes_read =
                                                    drain_bytes_read.saturating_add(bytes_read);
                                                drain_pending = true;
                                                drain_deferred = true;
                                                Ok(())
                                            }
                                            Err(error) => Err(error),
                                        }
                                    }
                                } else {
                                    // The fingerprint completed, which ends the rewrite. Whether the
                                    // reader still needs repositioning is decided inside, so a
                                    // further rewrite arriving before this one completed is not
                                    // mistaken for the one already rewound for.
                                    watcher
                                        .reconcile_rewrite(true, outcome.partial_prefix())
                                        .await
                                }
                            }
                            None => Ok(()),
                        };
                        if let Err(error) = reopened {
                            self.emitter.emit_file_watch_error(&tracked_path, error);
                            restart_failed = true;
                        }
                        if !same_inode {
                            // Only that arm either reopened the watcher on a different path or
                            // attempted the bounded drain that precedes it.
                            tracked_path_index.paths_changed();
                        }
                    }
                    if restart_failed {
                        // Leave it for the next pass rather than rekeying a reader that is still
                        // positioned in discarded content.
                    } else if drain_deferred {
                        // The old reader is still the owner of `stale_key`. The normal read loop can
                        // continue consuming it, and the next discovery pass will either finish
                        // the drain or apply the same bounded step again.
                    } else if rekey_watcher_with_drain(
                        fp_map,
                        checkpoints,
                        stale_key,
                        file_id,
                        drained_checkpoint,
                    ) {
                        tracked_path_index.rekeyed(stale_key, file_id);
                        let watcher = fp_map
                            .get_mut(&file_id)
                            .expect("just rekeyed this watcher into place");
                        watcher.set_file_findable(true);
                        // Rekeyed under the fingerprint it now has, so the rewrite is over.
                        watcher.fingerprint_completed();
                    } else {
                        // The new fingerprint belongs to another watcher (two files can share one),
                        // so this path cannot be rekeyed onto it. Leave it to the next pass rather
                        // than evicting a legitimate owner.
                        trace!(
                            message = "Fingerprint already owned by another watcher.",
                            path = ?path,
                        );
                    }
                } else {
                    // untracked file fingerprint
                    tracked_path_index.paths_changed();
                    self.watch_new_file(path, file_id, fp_map, checkpoints, false)
                        .await;
                    self.emit_open_and_idle_counts(fp_map);
                }
            } else {
                let stale_key = fp_map
                    .iter()
                    .find(|(_, watcher)| {
                        watcher.matches_path(&path, cwd_for_notify_comparison.as_deref())
                    })
                    .map(|(file_id, _)| *file_id);
                if let Some(stale_key) = stale_key {
                    let Some(watcher) = fp_map.get_mut(&stale_key) else {
                        continue;
                    };
                    // Fingerprinting can legitimately fail while a writer has only emitted a
                    // partial/short record. Active watchers are considered here too, not just idle
                    // ones: this pass has already marked every watcher unfindable, so skipping an
                    // active one leaves it `findable == false`, which `read_line` reads as "the file
                    // was deleted" -- and a later completed fingerprint then starts a *second* watcher
                    // on the same path, duplicating records.
                    // A path that is gone, or no longer a regular file, is left untouched: staying
                    // unfindable is what lets the grace period reap its watcher, and neither rewinding
                    // nor reopening it can succeed.
                    if path_absent {
                        trace!(message = "Watched path is gone or not a regular file.", path = ?path);
                    } else if !rewrite_suspected {
                        // A read error rather than an incomplete prefix: the file may be unchanged and
                        // readable, so rewinding would replay what the reader has emitted.
                        watcher.set_file_findable(true);
                    } else if watcher.path_has_tracked_identity().await {
                        // Same inode with a prefix that no longer hashes: an in-place rewrite. Reset the
                        // reader so it does not resume inside the new content, and keep the watcher
                        // findable while its new fingerprint is still incomplete. Whether this is the
                        // rewrite already rewound for, or a further one on top of it, is decided inside.
                        match watcher
                            .reconcile_rewrite(false, outcome.partial_prefix())
                            .await
                        {
                            Ok(()) => watcher.set_file_findable(true),
                            Err(error) => {
                                // The reopen failed -- typically the file was removed or replaced between
                                // the identity check and this open. Reporting it as found would keep a
                                // stale descriptor active and could let `remove_after` delete the
                                // replacement path; leave it unfindable for the normal recovery path.
                                self.emitter.emit_file_watch_error(&watcher.path, error);
                            }
                        }
                    } else if fs::metadata(&path)
                        .await
                        .is_ok_and(|metadata| metadata.is_file())
                    {
                        // A regular file is there but its identity does not match: the path was replaced.
                        // An idle watcher re-verifies identity on its next poll, but an active one never
                        // does -- left alone its reader stays on the old inode, missing the replacement
                        // until the backstop and leaving `remove_after` free to delete it meanwhile.
                        if watcher.is_idle() {
                            watcher.set_file_findable(true);
                            watcher.invalidate_idle_bookkeeping();
                        } else {
                            // A replacement can be too short for the configured fingerprinter. It is
                            // still a different inode, so reopening it immediately would discard the
                            // old reader's unread tail just because its new prefix is incomplete. Use
                            // the same bounded drain as the completed-fingerprint path and leave the
                            // old descriptor attached until it reaches EOF.
                            if drain_attempted.insert(stale_key) {
                                match drain_and_repoint(
                                    watcher,
                                    stale_key,
                                    path.clone(),
                                    lines,
                                    self.max_read_bytes,
                                )
                                .await
                                {
                                    Ok(DrainOutcome::Repointed { bytes_read, .. }) => {
                                        drain_bytes_read =
                                            drain_bytes_read.saturating_add(bytes_read);
                                        tracked_path_index.paths_changed();
                                    }
                                    Ok(DrainOutcome::LimitReached { bytes_read }) => {
                                        drain_bytes_read =
                                            drain_bytes_read.saturating_add(bytes_read);
                                        drain_pending = true;
                                    }
                                    Err(error) => {
                                        // `drain_and_repoint` restores the unfindable state on an
                                        // error, so the old descriptor remains available for a retry.
                                        self.emitter.emit_file_watch_error(&watcher.path, error);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        for (&file_id, watcher) in &mut *fp_map {
            watcher.finish_discovery();
            if watcher.take_reader_restarted() {
                // Any reader repositioned onto different content during this pass -- an in-place
                // rewrite, or a reopened same-name replacement -- must not leave the previous file's
                // offset persisted: a restart before the first new line is acknowledged would resume
                // past the new content's prefix. Swept here rather than after each individual
                // reposition, so no branch can forget it.
                checkpoints.register(file_id, 0, watcher.take_new_generation());
            }
        }
        DiscoveryOutcome {
            keep_notify_discovery,
            drain_pending,
            bytes_read: drain_bytes_read,
        }
    }

    /// Fingerprint only paths named by an ordinary notify modification event. The periodic full
    /// pass remains responsible for discovering creations, renames, removals, and dropped events.
    ///
    /// Returns `false` if some named path turned out to be neither a tracked fingerprint nor a
    /// tracked path. Some backends report a brand-new file as `Modify(Data)` with no create event,
    /// so that path may be an untracked file that belongs to the `include` set -- only the glob pass
    /// can decide that, and the caller must run one rather than leave the file unread until the
    /// reconciliation backstop.
    async fn discover_changed_paths(
        &mut self,
        paths: &HashSet<PathBuf>,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        known_small_files: &mut file_source_common::KnownSmallFiles,
        checkpoints: &CheckpointsView,
    ) -> bool {
        let mut all_paths_accounted_for = true;
        let cwd = std::env::current_dir().ok();
        // A watcher matches by its own path, or by this event path's canonical form: a file can be
        // included through a symlink alias, so notify reports the alias while `fp_map` holds the
        // real spelling and neither side of `matches_path` agrees. `NotifyWakeup::names` is
        // deliberately not used here -- it answers "is this path anywhere in the batch", which for a
        // multi-path batch would match every watcher against every path.
        // Resolving an event path to its watcher must not scan `fp_map`: that made the common
        // single-file write O(tracked files) per batch, the very sweep this targeted pass exists to
        // avoid. An index over both spellings a watcher answers to (its own path, absolutized, and
        // its cached canonical path) turns each resolution into a hash lookup.
        //
        // Built per pass rather than kept across passes: `fp_map` keys change under rekeying and
        // `watcher.path` changes under every `update_path`, so a persistent index would have to stay
        // in step with a dozen mutation sites including error paths, and silently resolves to the
        // wrong watcher whenever it drifts. One pass over `fp_map` to build it, then O(1) per event,
        // is the trade this makes -- and it borrows nothing from `fp_map`, which the loop below
        // needs mutably.
        let mut watcher_keys_by_path: HashMap<PathBuf, FileFingerprint> =
            HashMap::with_capacity(fp_map.len() * 2);
        for (&file_id, watcher) in &*fp_map {
            watcher_keys_by_path
                .entry(crate::absolutize(&watcher.path, cwd.as_deref()))
                .or_insert(file_id);
            if let Some(canonical) = watcher.canonical_path() {
                watcher_keys_by_path
                    .entry(canonical.to_path_buf())
                    .or_insert(file_id);
            }
        }
        let lookup_key = |canonical_event_path: Option<&Path>, path: &Path| {
            let absolutized = crate::absolutize(path, cwd.as_deref());
            watcher_keys_by_path
                .get(&absolutized)
                .or_else(|| canonical_event_path.and_then(|c| watcher_keys_by_path.get(c)))
                .copied()
        };

        for path in paths {
            // Resolved once per event path, not once per watcher. `None` when the path cannot be
            // canonicalized (it may already be gone), which simply leaves raw-path comparison.
            let canonical_event_path = fs::canonicalize(path).await.ok();
            let canonical_event_path = canonical_event_path.as_deref();

            // Notify watches whole *directories*, so an event names any file under them --
            // including ones the `include`/`exclude` rules leave out. Fingerprinting such a path
            // would add it to `known_small_files` when it is short or unterminated, and
            // `remove_after` then deletes a file the user explicitly excluded.
            //
            // Decided by matching this one path against the provider's rules: enumerating
            // `paths()` here would walk the whole include tree on every event, the very O(N) cost
            // this targeted pass exists to avoid. An already-tracked watcher vouches for its own
            // path, so a tracked file is never skipped if it briefly leaves the glob; a provider
            // that cannot decide cheaply (`None`) defers to the full pass instead of guessing.
            let tracked_key = lookup_key(canonical_event_path, path);
            let is_tracked = tracked_key.is_some();
            if !is_tracked {
                // Both spellings are tried, and either one matching is enough: a backend may report
                // the physical path (FSEvents resolves symlinks) while the include pattern is
                // written against the symlink, or the reverse. Checking only one would classify a
                // genuinely included file as excluded.
                // Only genuinely distinct spellings are consulted, and a missing second spelling is
                // *not* an undecided verdict: canonicalization usually returns the same path, and
                // treating that as "undecided" forced a full glob pass for every unrelated change
                // in a watched directory -- exactly the sweep this path exists to avoid.
                let mut verdicts = vec![self.paths_provider.is_included(path)];
                if let Some(canonical) = canonical_event_path.filter(|c| *c != path) {
                    verdicts.push(self.paths_provider.is_included(canonical));
                }
                // A pattern written through a symlink (`/var/log/app` -> `/mnt/disk/app`) matches
                // neither the physical path a backend like FSEvents reports nor its canonical form,
                // so string matching alone would silently discard a genuinely included new file.
                // Comparing canonical *roots* catches that without forcing a pass for every
                // unrelated change: only an event under one of this provider's watch roots, by
                // identity rather than by spelling, is handed to the full pass.
                let reached_through_a_symlinked_root = !verdicts.contains(&Some(true))
                    && match canonical_event_path {
                        Some(canonical) => {
                            let mut under_a_root = false;
                            for root in self.paths_provider.watch_roots() {
                                let root = crate::absolutize(&root, cwd.as_deref());
                                // Patterns carry wildcards; their leading literal directory is what
                                // can be canonicalized.
                                // Reuses `compute_watch_directories`' metacharacter set: checking
                                // only `*` and `?` picked `/link/[a-z]` as a "literal" root, which
                                // cannot be canonicalized, so a symlinked root followed by a
                                // bracket or brace component was never recognised.
                                let literal_root = root
                                    .ancestors()
                                    .find(|ancestor| {
                                        ancestor.to_str().is_some_and(|ancestor| {
                                            !crate::notify_watcher::contains_glob_metachar(ancestor)
                                        })
                                    })
                                    .map(Path::to_path_buf);
                                let Some(literal_root) = literal_root else {
                                    continue;
                                };
                                let Ok(canonical_root) = fs::canonicalize(&literal_root).await
                                else {
                                    continue;
                                };
                                if canonical.starts_with(&canonical_root) {
                                    if path_contains_symlink(&literal_root).await {
                                        // The literal prefix itself is reached through a symlink, so
                                        // no spelling comparison can settle membership.
                                        //
                                        // Asked of the filesystem rather than by comparing the
                                        // prefix against its canonical form: canonicalization
                                        // rewrites a path with no symlink involved at all -- a
                                        // Windows 8.3 short name (`6DB9~1`) becomes the long name
                                        // under `\\?\`, macOS `/var` becomes `/private/var` -- so
                                        // treating inequality as "symlink" made every excluded
                                        // sibling in a watched directory force a full glob pass on
                                        // those platforms.
                                        under_a_root = true;
                                        break;
                                    }
                                    // The event is already under the real literal root. A child
                                    // link cannot be the reason it reaches this path, so do not
                                    // walk the root tree for every ordinary excluded sibling.
                                    continue;
                                }
                                // The symlink can instead sit in a *wildcard* component, and then
                                // the event path need not be under the literal prefix at all: for
                                // `<root>/*/*.log` where `<root>/linked` points outside the include
                                // tree, a backend reporting physical paths (FSEvents) names a path
                                // that matches no spelling and lies under no watch root. Testing
                                // only the prefix dropped such an event as accounted for, leaving
                                // the file unread until the reconciliation backstop.
                                //
                                // The walk is bounded by the pattern's shape (see
                                // `symlink_search_depth`) and runs only for an untracked path that
                                // matched no spelling, so it costs a bounded slice of the tree on an
                                // event that was otherwise about to be discarded.
                                if root != literal_root {
                                    // `None` means the depth cap stopped the walk early, so a link may
                                    // sit deeper. Treated like a hit: both hand the event to the full
                                    // pass, which is the only thing that can settle it.
                                    match path_reachable_through_child_link(
                                        &literal_root,
                                        canonical,
                                        symlink_search_depth(&root, &literal_root),
                                    )
                                    .await
                                    {
                                        Some(true) | None => {
                                            under_a_root = true;
                                            break;
                                        }
                                        Some(false) => {}
                                    }
                                }
                            }
                            under_a_root
                        }
                        None => false,
                    };
                if verdicts.contains(&Some(true)) {
                    // Included under at least one spelling.
                } else if verdicts.contains(&None) || reached_through_a_symlinked_root {
                    // Either the provider cannot decide membership cheaply, or the path reaches a
                    // watch root only through a symlink, so no spelling comparison can settle it.
                    // The path may well be a new file the provider would yield: let the full pass
                    // decide rather than discarding the event.
                    all_paths_accounted_for = false;
                    continue;
                } else {
                    continue;
                }
            }

            // Fingerprinted under the spelling the *configuration* named, not the event's. A backend
            // like FSEvents reports a symlinked include's canonical target, and `KnownSmallFiles`
            // takes its first spelling as the one `remove_after` may unlink -- so passing the event
            // path here would let it delete the target, outside the include and possibly shared.
            // Only for a tracked file: an untracked path has no configured spelling to prefer, and
            // the glob pass records it under one when it runs.
            let configured_spelling = tracked_key
                .and_then(|key| fp_map.get(&key))
                .map(|watcher| watcher.path.clone());
            let fingerprint_path = configured_spelling.as_deref().unwrap_or(path);
            let outcome = self
                .fingerprinter
                .fingerprint_or_emit_detailed(
                    fingerprint_path,
                    known_small_files,
                    &self.emitter,
                    if tracked_key.is_some() {
                        PrefixWanted::Yes
                    } else {
                        PrefixWanted::No
                    },
                )
                .await;
            let rewrite_suspected = outcome.is_incomplete();
            let Some(file_id) = outcome.fingerprint() else {
                if let Some(watcher) = tracked_key.and_then(|key| fp_map.get_mut(&key)) {
                    if watcher.is_idle() {
                        // Preserve an idle watcher through a transient short/partial fingerprint
                        // failure, and also allow a same-name replacement to be reopened safely:
                        // `reactivate` verifies identity before deciding whether to reset offset.
                        if !rewrite_suspected {
                            // An I/O or decode error, not an incomplete prefix. The file may be
                            // unchanged and readable, so rewinding would replay emitted lines.
                            watcher.mark_found();
                        } else if watcher.path_has_tracked_identity().await {
                            // Same inode, yet its prefix no longer hashes: an in-place rewrite.
                            // Rewinding resets the offset an idle watcher would otherwise seek to --
                            // without it, reactivation sees no shrink (the partial prefix can
                            // already exceed the old offset) and resumes inside the rewritten
                            // content. Only a watcher that actually restarted counts as accounted
                            // for: the gzip re-probe opens the file and can fail after the
                            // fingerprint already did.
                            match watcher
                                .reconcile_rewrite(false, outcome.partial_prefix())
                                .await
                            {
                                Ok(()) => watcher.mark_found(),
                                Err(error) => {
                                    self.emitter.emit_file_watch_error(&watcher.path, error);
                                    all_paths_accounted_for = false;
                                }
                            }
                        } else if fs::metadata(path)
                            .await
                            .is_ok_and(|metadata| metadata.is_file())
                        {
                            // A different file is at this path, or identity is indeterminate. Keep
                            // the watcher alive through a transient short read, and let the idle poll
                            // re-verify identity before deciding where to resume.
                            watcher.mark_found();
                            watcher.invalidate_idle_bookkeeping();
                        } else {
                            // The path is gone: neither identity nor a plain file remains. Removal
                            // and rename recovery are the full pass's job, and leaving the watcher
                            // findable here would defer both until the backstop.
                            all_paths_accounted_for = false;
                        }
                    } else if crate::file_watcher::path_is_absent(path).await {
                        // A queued event can be processed after its file is already gone. Marking
                        // the watcher found here would claim the path is accounted for and skip the
                        // full pass, leaving an active watcher attached to the old inode until the
                        // backstop; removal and replacement recovery are the full pass's job.
                        all_paths_accounted_for = false;
                    } else {
                        if !watcher.path_has_tracked_identity().await {
                            // A targeted pass has no room to drain a different inode. Defer to the
                            // full pass, which can drain the old descriptor before opening a short or
                            // fully fingerprintable replacement.
                            all_paths_accounted_for = false;
                        } else if rewrite_suspected {
                            // Same inode, yet its prefix no longer hashes: an in-place rewrite, so
                            // the reader's offset and buffered bytes describe vanished content.
                            // `rewrite_suspected` is required: a transient read error says nothing
                            // about the content, and rewinding on one replays what was emitted.
                            //
                            // No size check: with a multi-line or header-skipping strategy the
                            // partially rewritten prefix can already be *longer* than the reader's
                            // offset while still being too short to hash, so requiring a shrink
                            // missed exactly those rewrites and spliced the new content onto a
                            // stale tail.
                            if let Err(error) = watcher
                                .reconcile_rewrite(false, outcome.partial_prefix())
                                .await
                            {
                                self.emitter.emit_file_watch_error(&watcher.path, error);
                            }
                            // The full pass re-fingerprints this file once its prefix is complete,
                            // which is what rekeys it.
                            all_paths_accounted_for = false;
                        } else {
                            watcher.mark_found();
                            watcher.mark_ready_to_read();
                        }
                    }
                } else {
                    // An unfingerprintable path nothing tracks: could be a short brand-new file.
                    all_paths_accounted_for = false;
                }
                continue;
            };

            // The fingerprint can belong to a *different* file: two files whose fingerprinted
            // prefixes are identical share one `FileFingerprint`. Only treat the hit as this
            // path's watcher once the path agrees; otherwise fall through to the path lookup
            // below, or the watcher that actually owns this path would not be woken until the
            // next full backstop pass.
            let fingerprint_matched_this_path = tracked_key == Some(file_id);

            if fingerprint_matched_this_path {
                let watcher = fp_map
                    .get_mut(&file_id)
                    .expect("just checked this fingerprint is present");
                // A notify-targeted pass must not reopen a same-named replacement in place: doing
                // so discards the old descriptor before the full pass has had a chance to drain
                // its tail. Leave the event unaccounted for and let `discover` perform the bounded
                // rotation handoff.
                if !watcher
                    .candidate_has_tracked_identity(watcher.path.clone())
                    .await
                {
                    all_paths_accounted_for = false;
                    continue;
                }
                let mut refresh_failed = false;
                if watcher.is_active() {
                    let check = watcher.shrank_below_reader().await;
                    // Appended gzip members consumed since the reader opened leave its baseline
                    // below what was actually read, so raise it from the stat just taken.
                    watcher.observe_raw_size(check.observed);
                    if check.shrank {
                        // Truncated and rewritten in place while keeping the same fingerprint -- the
                        // rewrite reused the first line, so there is nothing to rekey, but the reader
                        // is still positioned in content that no longer exists. This is the ordinary
                        // `copytruncate` shape for a log whose header does not change.
                        if let Err(error) = watcher.restart_after_rewrite().await {
                            self.emitter.emit_file_watch_error(&watcher.path, error);
                            refresh_failed = true;
                        } else {
                            // The reader restarted at zero, so the persisted position must not keep
                            // pointing past the start of the rewritten file.
                            checkpoints.register(file_id, 0, watcher.take_new_generation());
                        }
                    }
                }
                if refresh_failed {
                    // See above: a failed reopen must not be reported as accounted for.
                    all_paths_accounted_for = false;
                } else {
                    watcher.mark_found();
                    watcher.mark_ready_to_read();
                }
                // The file fingerprints again, so the rewrite is over. Left set, the *next* rewrite
                // would be mistaken for this one and skip its repositioning.
                watcher.fingerprint_completed();
                if watcher.is_idle() && !watcher.path_has_tracked_identity().await {
                    watcher.invalidate_idle_bookkeeping();
                }
            } else if let Some(stale_key) = tracked_key {
                // Tracked, but under a fingerprint it no longer has: an in-place rewrite. Rekey so
                // lines are checkpointed under the file's real identity and the next full pass does
                // not start a second watcher on the same path.
                //
                // Occupancy first, before the reader is touched: `rekey_watcher` refuses a
                // fingerprint another watcher owns, and restarting first would leave this reader
                // rewound onto the new content while still filed under the stale key.
                if stale_key != file_id && fp_map.contains_key(&file_id) {
                    all_paths_accounted_for = false;
                    continue;
                }
                let Some(watcher) = fp_map.get_mut(&stale_key) else {
                    // Not `expect`: an earlier path in this batch may already have rekeyed it, since
                    // two spellings of one inode both resolve to the now-removed key.
                    continue;
                };
                // A changed fingerprint can also be caused by a same-path replacement. The
                // targeted pass has no room for the bounded old-inode drain, so defer this event to
                // the full pass instead of rekeying a reader that is still attached to the old
                // descriptor.
                if !watcher
                    .candidate_has_tracked_identity(watcher.path.clone())
                    .await
                    || !watcher.path_has_tracked_identity().await
                {
                    all_paths_accounted_for = false;
                    continue;
                }
                // No size check: under `FirstLinesChecksum` a changed fingerprint on the same inode
                // can only be an in-place rewrite, and requiring a shrink missed the rewrites larger
                // than the reader's offset.
                // The fingerprint completed, which ends the rewrite; whether the reader still needs
                // repositioning is decided inside, so a further rewrite arriving before this one
                // completed is not mistaken for the one already rewound for. The identity check
                // gates only the reopen -- the guard must come down either way, or the *next*
                // rewrite inherits it and skips its repositioning.
                if let Err(error) = watcher
                    .reconcile_rewrite(true, outcome.partial_prefix())
                    .await
                {
                    self.emitter.emit_file_watch_error(&watcher.path, error);
                    all_paths_accounted_for = false;
                    continue;
                }
                if !rekey_watcher(fp_map, checkpoints, stale_key, file_id) {
                    all_paths_accounted_for = false;
                    continue;
                }
                let watcher = fp_map
                    .get_mut(&file_id)
                    .expect("just rekeyed this watcher into place");
                // The rewrite completed under its new fingerprint, so it is over.
                watcher.fingerprint_completed();
                if watcher.is_idle() {
                    if watcher.path_has_tracked_identity().await
                        || fs::metadata(path)
                            .await
                            .is_ok_and(|metadata| metadata.is_file())
                    {
                        watcher.mark_found();
                        watcher.invalidate_idle_bookkeeping();
                    }
                } else {
                    let mut refresh_failed = false;
                    if watcher.is_active() && !watcher.path_has_tracked_identity().await {
                        let current_path = watcher.path.clone();
                        if let Err(error) = watcher.update_path(current_path).await {
                            self.emitter.emit_file_watch_error(&watcher.path, error);
                            refresh_failed = true;
                        }
                    }
                    if refresh_failed {
                        // See above: a failed reopen must not be reported as accounted for.
                        all_paths_accounted_for = false;
                    } else {
                        watcher.mark_found();
                        watcher.mark_ready_to_read();
                    }
                }
            } else {
                // Nothing tracks this path under either its fingerprint or its name. It may be a
                // newly created file that the `include` globs cover, reported without a create
                // event; only the glob pass can tell.
                all_paths_accounted_for = false;
            }
        }

        // Same sweep as at the end of the full pass, because this one returns before reaching it:
        // a reader repositioned onto different content must not leave the previous file's offset
        // persisted, or a restart resumes past the new content's prefix.
        for (&file_id, watcher) in &mut *fp_map {
            if watcher.take_reader_restarted() {
                checkpoints.register(file_id, 0, watcher.take_new_generation());
            }
        }

        all_paths_accounted_for
    }

    /// Whether at least one idle watcher has reached its independent removal deadline.
    fn idle_removal_due(&self, fp_map: &IndexMap<FileFingerprint, FileWatcher>) -> bool {
        let Some(grace_period) = self.remove_after else {
            return false;
        };
        fp_map.values().any(|watcher| {
            watcher
                .idle_removal_delay(grace_period)
                .is_some_and(|delay| delay.is_zero())
        })
    }

    /// Return the shortest remaining idle-removal delay, if any idle watcher is being retained.
    ///
    /// This is a timer calculation only: it does not inspect the filesystem, which keeps the
    /// notify path from turning one unrelated event into an O(number-of-idle-watchers) metadata
    /// sweep.
    fn next_idle_removal_delay(
        &self,
        fp_map: &IndexMap<FileFingerprint, FileWatcher>,
    ) -> Option<Duration> {
        let grace_period = self.remove_after?;
        fp_map
            .values()
            .filter_map(|watcher| watcher.idle_removal_delay(grace_period))
            .min()
    }

    /// Remove only idle watchers whose deadline is due, after checking each file once more for
    /// delayed or dropped notify events. This pass exists so a notify source can reap an unchanged
    /// idle file without waiting for the reconciliation backstop.
    async fn remove_idle_watchers_due(
        &self,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        lines: &mut Vec<Line>,
    ) {
        for (&file_id, watcher) in &mut *fp_map {
            self.recheck_idle_watcher_before_removal(watcher, file_id, lines)
                .await;
        }
    }

    /// Recheck an idle watcher's file before applying its removal deadline. A notify event can be
    /// delayed or dropped, so the deadline alone is not enough evidence that the file is unchanged.
    async fn recheck_idle_watcher_before_removal(
        &self,
        watcher: &mut FileWatcher,
        file_id: FileFingerprint,
        lines: &mut Vec<Line>,
    ) {
        let Some(grace_period) = self.remove_after else {
            return;
        };
        if watcher
            .idle_removal_delay(grace_period)
            .is_none_or(|delay| !delay.is_zero())
        {
            return;
        }
        match watcher.check_for_new_data().await {
            Ok(true) => {
                if let Err(error) = watcher.reactivate().await {
                    self.emitter.emit_file_watch_error(&watcher.path, error);
                    // The failed reactivation must be retried, not treated as an unchanged file
                    // on the next timer tick.
                    watcher.invalidate_idle_bookkeeping();
                }
            }
            Ok(false) => {
                self.remove_idle_watcher_if_due(watcher, file_id, lines)
                    .await;
            }
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    self.emitter.emit_file_watch_error(&watcher.path, error);
                }
                watcher.defer_idle_removal(IDLE_REMOVAL_RETRY_INTERVAL);
            }
        }
    }

    /// Cheaply poll `Idle` watchers (no open file handle) for new data by stat-ing them, reusing
    /// the same discovery cadence (`discover`'s caller) rather than adding a whole separate
    /// polling loop. Promotes any that changed back to `Active` so the read loop picks them up.
    ///
    /// Normally skips watchers that `discover`'s glob/fingerprint pass marked unfindable. An
    /// `Idle` watcher holds no handle, so reopening its old path could attach the stale checkpoint
    /// to a replacement file. The exception is a watcher whose original inode was located at a
    /// path outside the glob; notify event paths are checked for every unfindable watcher so
    /// repeated rotations remain recoverable. A broad scan is also used for an outside-glob
    /// watcher when there is no precise rename candidate (polling, overflow, or an empty event)
    /// or when the candidate set was truncated. Once the path is identity-verified, it is safe to
    /// continue polling, preserving data appended after rotation.
    async fn poll_idle_watchers(
        &self,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        lines: &mut Vec<Line>,
        notify_wakeup: &NotifyWakeup,
    ) {
        let event_identities = match notify_wakeup.named_paths() {
            Some(paths) => Some(identify_event_paths(paths).await),
            None => None,
        };
        let cwd_for_notify_comparison = notify_wakeup
            .has_specific_paths()
            .then(|| std::env::current_dir().ok())
            .flatten();
        let mut tree_identities_by_root = HashMap::new();

        for (&file_id, watcher) in &mut *fp_map {
            if !watcher.is_idle() {
                continue;
            }

            let mut path_has_tracked_identity = false;
            if !watcher.file_findable() {
                // Event paths are precise and cheap to inspect. Check them even after this
                // watcher was moved outside the glob: a later rotation can move that same inode
                // again. Scan the parent tree when raw rename candidates cannot identify this
                // watcher, or when polling/coarse reconciliation requires a fallback. For an
                // outside-glob watcher, first avoid that scan while its current path still has
                // the tracked identity (unless an explicit rename candidate is pending, because
                // the event can arrive before the filesystem rename has finished); the scan is
                // still shared per root for the whole pass.
                let path_outside_glob = watcher.path_is_outside_glob();
                let broad_scan_required = self.discovery_mode == FileDiscoveryMode::PollingOnly
                    || notify_wakeup.requires_broad_rename_scan();
                // With a coarse/polling pass there is no precise rename candidate to resolve. A
                // cheap identity check lets an outside-glob watcher skip the parent-tree scan when
                // its last verified path is still the same. Do not perform this stat for an
                // ordinary targeted event: it cannot help a watcher the event did not name.
                if path_outside_glob && broad_scan_required && event_identities.is_none() {
                    path_has_tracked_identity = watcher.path_has_tracked_identity().await;
                }
                let path = if let Some(path) =
                    watcher.find_renamed_path_in_identities(event_identities.as_ref())
                {
                    path_has_tracked_identity = path_outside_glob;
                    Some(path)
                } else if broad_scan_required
                    && (!path_outside_glob
                        || event_identities.is_some()
                        || !path_has_tracked_identity)
                {
                    let Some(root) = watcher.rename_search_root() else {
                        continue;
                    };
                    if !tree_identities_by_root.contains_key(&root) {
                        tree_identities_by_root
                            .insert(root.clone(), identify_paths_in_tree(&root).await);
                    }
                    watcher
                        .find_renamed_path_with_identities(None, tree_identities_by_root.get(&root))
                        .await
                } else {
                    None
                };

                if let Some(path) = path {
                    if let Err(error) = watcher.update_path(path).await {
                        self.emitter.emit_file_watch_error(&watcher.path, error);
                    } else {
                        watcher.mark_path_outside_glob();
                        path_has_tracked_identity = true;
                    }
                } else if broad_scan_required
                    && path_outside_glob
                    && !path_has_tracked_identity
                    && watcher.tracked_file_is_gone().await
                {
                    // The previously identity-verified outside-glob path disappeared and no
                    // replacement was found. It is no longer safe to keep this watcher on the
                    // long rotate-wait grace; let the normal finite missing-path grace reap it.
                    //
                    // Absence is confirmed separately: `path_has_tracked_identity` is also false
                    // when the identity read failed for a permission, sharing, or I/O reason, and
                    // treating that as deletion would stop polling a rotated file and reap it after
                    // `discovery_interval`, losing data appended while it was unreadable.
                    watcher.clear_path_outside_glob();
                }
            }

            if !idle_watcher_can_be_polled(
                watcher.file_findable(),
                watcher.path_is_outside_glob(),
                path_has_tracked_identity,
            ) {
                continue;
            }

            let notify_names_current_path = notify_wakeup.has_specific_paths()
                && notify_wakeup.names(
                    &crate::absolutize(&watcher.path, cwd_for_notify_comparison.as_deref()),
                    watcher.canonical_path(),
                );

            // A wakeup that names paths says nothing about the files it does not name, so statting
            // them is the per-file-per-pass cost this mode exists to avoid: one busy writer would
            // otherwise sweep every idle file on every event. The backstop pass names nothing and so
            // still sweeps everything, which is what covers an event the backend dropped.
            //
            // Their removal deadline is still due, and reading a timer costs nothing.
            if notify_wakeup.has_specific_paths() && !notify_names_current_path {
                self.recheck_idle_watcher_before_removal(watcher, file_id, lines)
                    .await;
                continue;
            }

            match watcher.check_for_new_data().await {
                Ok(true) => {
                    if let Err(error) = watcher.reactivate().await {
                        self.emitter.emit_file_watch_error(&watcher.path, error);
                        // check_for_new_data already recorded the size/mtime it just observed
                        // before we got here. Without this, a transient reactivate() failure
                        // (the file exists and changed, per the stat we just did, but couldn't be
                        // opened for some other reason) would strand the watcher: the next poll
                        // would compare against the state recorded from *this* failed attempt,
                        // see no further difference, and never retry.
                        watcher.invalidate_idle_bookkeeping();
                    } else {
                        debug!(
                            message = "Idle file has new data; resuming active watch.",
                            path = ?watcher.path,
                        );
                    }
                }
                Ok(false) => {
                    if notify_names_current_path {
                        // Size and mtime are not a complete replacement detector: a new inode can
                        // coincidentally reuse both values. A concrete notify event for this exact
                        // path justifies one safe reopen; `reactivate` verifies the identity and
                        // resets the offset if the file was replaced.
                        if let Err(error) = watcher.reactivate().await {
                            if error.kind() != std::io::ErrorKind::NotFound {
                                self.emitter.emit_file_watch_error(&watcher.path, error);
                            }
                            watcher.invalidate_idle_bookkeeping();
                        }
                        continue;
                    }
                    // Still idle and still unchanged, so its removal deadline may now be due.
                    self.remove_idle_watcher_if_due(watcher, file_id, lines)
                        .await;
                }
                Err(error) => {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        // Deletion of idle files is handled uniformly below via
                        // `file_findable`/`rotate_wait`, so nothing more to do here; the next
                        // discovery pass will mark this watcher unfindable.
                    } else {
                        self.emitter.emit_file_watch_error(&watcher.path, error);
                    }
                }
            }
        }
    }

    /// Emit the `files_open`/`files_idle` gauges from the current contents of `fp_map`.
    /// `files_open` reflects only watchers that actually hold an open file handle. This is not
    /// identical to the logical `Active` state: a gzip watcher configured to skip its backlog is
    /// active with a null reader and therefore belongs in `files_idle`. Prior to the idle-watching
    /// feature these were always identical to `fp_map.len()`; splitting them out is what makes the fix for
    /// <https://github.com/vectordotdev/vector/issues/3567> observable.
    fn emit_open_and_idle_counts(&self, fp_map: &IndexMap<FileFingerprint, FileWatcher>) {
        let (mut open, mut idle) = (0usize, 0usize);
        for watcher in fp_map.values() {
            if watcher.holds_file_handle() {
                open += 1;
            } else {
                idle += 1;
            }
        }
        self.emitter.emit_files_open(open);
        self.emitter.emit_files_idle(idle);
    }

    /// Delete an idle file that has been quiet for `remove_after`, if it is due.
    ///
    /// Driven off how long the watcher has sat unchanged rather than "time since last successful
    /// read", which means nothing for a watcher that by construction is not reading. Reading that
    /// timer costs no syscall, so this is also safe to run for watchers a notify wakeup did not
    /// name -- their deadline falls due on schedule rather than waiting for the backstop pass.
    async fn remove_idle_watcher_if_due(
        &self,
        watcher: &mut FileWatcher,
        file_id: FileFingerprint,
        lines: &mut Vec<Line>,
    ) {
        let Some(grace_period) = self.remove_after else {
            return;
        };
        if watcher
            .idle_removal_delay(grace_period)
            .is_none_or(|delay| !delay.is_zero())
        {
            return;
        }
        if !watcher.removal_is_authorized() {
            // Drained, not deleted: this path is outside the include patterns.
            salvage_final_partial_line(watcher, file_id, lines);
            watcher.set_dead();
            return;
        }
        match remove_file(&watcher.path).await {
            Ok(()) => {
                self.emitter.emit_file_deleted(&watcher.path);
                salvage_final_partial_line(watcher, file_id, lines);
                watcher.set_dead();
            }
            Err(error) => {
                self.emitter.emit_file_delete_error(&watcher.path, error);
                watcher.defer_idle_removal(IDLE_REMOVAL_RETRY_INTERVAL);
            }
        }
    }

    async fn watch_new_file(
        &self,
        path: PathBuf,
        file_id: FileFingerprint,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        checkpoints: &CheckpointsView,
        startup: bool,
    ) {
        // Determine the initial _requested_ starting point in the file. This can be overridden
        // once the file is actually opened and we determine it is compressed, older than we're
        // configured to read, etc.
        let fallback = if startup {
            self.read_from
        } else {
            // Always read new files that show up while we're running from the beginning. There's
            // not a good way to determine if they were moved or just created and written very
            // quickly, so just make sure we're not missing any data.
            ReadFrom::Beginning
        };

        // Always prefer the stored checkpoint unless the user has opted out.  Previously, the
        // checkpoint was only loaded for new files when Vector was started up, but the
        // `kubernetes_logs` source returns the files well after start-up, once it has populated
        // them from the k8s metadata, so we now just always use the checkpoints unless opted out.
        // https://github.com/vectordotdev/vector/issues/7139
        let read_from = if !self.ignore_checkpoints {
            checkpoints
                .get(file_id)
                .map(ReadFrom::Checkpoint)
                .unwrap_or(fallback)
        } else {
            fallback
        };

        match FileWatcher::new(
            path.clone(),
            read_from,
            self.ignore_before,
            self.max_line_bytes,
            self.line_delimiter.clone(),
            self.idle_timeout.is_some(),
        )
        .await
        {
            Ok(mut watcher) => {
                if let ReadFrom::Checkpoint(file_position) = read_from {
                    self.emitter.emit_file_resumed(&path, file_position);
                } else {
                    self.emitter.emit_file_added(&path);
                }
                watcher.set_file_findable(true);
                // Before the watcher can be read from, so its very first checkpoint is accepted. A
                // checkpoint loaded from disk has no owner, and one left over from a previous owner
                // of this fingerprint belongs to a different reader; either way every update would
                // be refused until this runs.
                checkpoints.register(file_id, watcher.get_file_position(), watcher.generation());
                fp_map.insert(file_id, watcher);
            }
            Err(error) => self.emitter.emit_file_watch_error(&path, error),
        };
    }
}

async fn checkpoint_writer(
    checkpointer: Checkpointer,
    sleep_duration: Duration,
    mut shutdown: impl Future + Unpin,
    emitter: impl FileSourceInternalEvents,
) -> Arc<Checkpointer> {
    let checkpointer = Arc::new(checkpointer);
    loop {
        let sleep = sleep(sleep_duration);
        tokio::select! {
            _ = &mut shutdown => break,
            _ = sleep => {},
        }

        let emitter = emitter.clone();
        let checkpointer = Arc::clone(&checkpointer);
        let start = time::Instant::now();
        match checkpointer.write_checkpoints().await {
            Ok(count) => emitter.emit_file_checkpointed(count, start.elapsed()),
            Err(error) => emitter.emit_file_checkpoint_write_error(error),
        };
    }
    checkpointer
}

pub fn calculate_ignore_before(ignore_older_secs: Option<u64>) -> Option<DateTime<Utc>> {
    ignore_older_secs.map(|secs| Utc::now() - chrono::Duration::seconds(secs as i64))
}

/// A sentinel type to signal that file server was gracefully shut down.
///
/// The purpose of this type is to clarify the semantics of the result values
/// returned from the [`FileServer::run`] for both the users of the file server,
/// and the implementors.
#[derive(Debug)]
pub struct Shutdown;

struct TimingStats {
    started_at: time::Instant,
    segments: BTreeMap<&'static str, Duration>,
    events: usize,
    bytes: usize,
}

impl TimingStats {
    fn record(&mut self, key: &'static str, duration: Duration) {
        let segment = self.segments.entry(key).or_default();
        *segment += duration;
    }

    fn record_bytes(&mut self, bytes: usize) {
        self.events += 1;
        self.bytes += bytes;
    }

    fn report(&self) {
        if !tracing::level_enabled!(tracing::Level::DEBUG) {
            return;
        }
        let total = self.started_at.elapsed();
        let counted: Duration = self.segments.values().sum();
        let other: Duration = total.saturating_sub(counted);
        let mut ratios = self
            .segments
            .iter()
            .map(|(k, v)| (*k, v.as_secs_f32() / total.as_secs_f32()))
            .collect::<BTreeMap<_, _>>();
        ratios.insert("other", other.as_secs_f32() / total.as_secs_f32());
        let (event_throughput, bytes_throughput) = if total.as_secs() > 0 {
            (
                self.events as u64 / total.as_secs(),
                self.bytes as u64 / total.as_secs(),
            )
        } else {
            (0, 0)
        };
        debug!(event_throughput = %scale(event_throughput), bytes_throughput = %scale(bytes_throughput), ?ratios);
    }
}

fn scale(bytes: u64) -> String {
    let units = ["", "k", "m", "g"];
    let mut bytes = bytes as f32;
    let mut i = 0;
    while bytes > 1000.0 && i <= 3 {
        bytes /= 1000.0;
        i += 1;
    }
    format!("{:.3}{}/sec", bytes, units[i])
}

impl Default for TimingStats {
    fn default() -> Self {
        Self {
            started_at: time::Instant::now(),
            segments: Default::default(),
            events: Default::default(),
            bytes: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct Line {
    pub text: Bytes,
    pub filename: String,
    pub file_id: FileFingerprint,
    /// Which watcher read this line, captured here rather than looked up when the checkpoint is
    /// written: by then the file may have been rekeyed, and the fingerprint alone cannot tell a
    /// live reader's progress from a previous owner's late acknowledgement.
    pub generation: OwnerGeneration,
    pub start_offset: u64,
    pub end_offset: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_wakeup_starts_none_and_reports_not_pending() {
        let wakeup = NotifyWakeup::default();
        assert!(!wakeup.is_pending());
        assert!(!wakeup.names(&PathBuf::from("/var/log/a.log"), None));
    }

    #[test]
    fn notify_wakeup_names_only_the_specific_paths_added() {
        // Regression test for a bug found in review: a single notify event must not cause
        // `discover` to nudge every tracked watcher's read pacing -- only the watcher(s) whose
        // path the event actually named. Otherwise one changed file among many thousands turns
        // into an O(N) sweep on every single event.
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([PathBuf::from("/var/log/a.log")]);

        assert!(wakeup.is_pending());
        assert!(wakeup.names(&PathBuf::from("/var/log/a.log"), None));
        assert!(
            !wakeup.names(&PathBuf::from("/var/log/b.log"), None),
            "a path the event didn't name must not be reported as needing a nudge"
        );
    }

    #[test]
    fn outside_glob_idle_watcher_requires_identity_before_polling() {
        assert!(idle_watcher_can_be_polled(true, true, false));
        assert!(idle_watcher_can_be_polled(false, true, true));
        assert!(!idle_watcher_can_be_polled(false, true, false));
        assert!(!idle_watcher_can_be_polled(false, false, true));
    }

    #[test]
    fn notify_wakeup_accumulates_paths_across_multiple_add_calls() {
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([PathBuf::from("/var/log/a.log")]);
        wakeup.add_paths([PathBuf::from("/var/log/b.log")]);

        assert!(wakeup.names(&PathBuf::from("/var/log/a.log"), None));
        assert!(wakeup.names(&PathBuf::from("/var/log/b.log"), None));
        assert!(!wakeup.names(&PathBuf::from("/var/log/c.log"), None));
    }

    #[test]
    fn notify_wakeup_mark_all_names_everything() {
        // `Overflow`/`BackendError` don't carry specific paths, so every tracked watcher must be
        // treated as possibly needing a nudge -- this is the pre-existing coarse behavior,
        // preserved for the cases where no finer-grained information is available.
        let mut wakeup = NotifyWakeup::default();
        wakeup.mark_all();

        assert!(wakeup.is_pending());
        assert!(wakeup.names(&PathBuf::from("/var/log/anything.log"), None));
    }

    #[test]
    fn notify_wakeup_falls_back_to_all_past_the_path_limit() {
        // Bounds the memory (and, in `discover`, the per-watcher `HashSet` lookup cost) a burst of
        // events touching many distinct paths can accumulate: past `NOTIFY_WAKEUP_PATH_LIMIT`,
        // tracking individual paths stops being worth it and `NotifyWakeup` falls back to `All`.
        let mut wakeup = NotifyWakeup::default();
        let many_paths =
            (0..=NOTIFY_WAKEUP_PATH_LIMIT).map(|i| PathBuf::from(format!("/var/log/{i}.log")));
        wakeup.add_paths(many_paths);

        assert!(matches!(&wakeup.state, NotifyWakeupState::All));
        assert!(wakeup.names(&PathBuf::from("/var/log/anything-else.log"), None));
    }

    #[test]
    fn notify_wakeup_keeps_rename_paths_when_change_path_limit_is_exceeded() {
        let mut wakeup = NotifyWakeup::default();
        let destination = PathBuf::from("/var/log/archive/app.log.1");
        wakeup.add_removed_paths([destination.clone()]);
        wakeup.add_paths(
            (0..=NOTIFY_WAKEUP_PATH_LIMIT).map(|i| PathBuf::from(format!("/var/log/{i}.log"))),
        );

        assert!(matches!(&wakeup.state, NotifyWakeupState::All));
        assert_eq!(
            wakeup
                .named_paths()
                .and_then(|paths| paths.get(&destination)),
            Some(&destination),
            "rename candidates must survive the coarse change-path wakeup"
        );
    }

    #[test]
    fn notify_wakeup_does_not_use_ordinary_change_paths_for_rename_recovery() {
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([PathBuf::from("/var/log/app.log")]);

        assert!(
            wakeup.named_paths().is_none(),
            "ordinary writes must not cause every idle watcher to scan their event path"
        );
    }

    #[test]
    fn broad_rename_scan_is_used_when_precise_candidates_are_unavailable() {
        let wakeup = NotifyWakeup::default();
        assert!(wakeup.requires_broad_rename_scan());

        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([PathBuf::from("/var/log/app.log")]);
        assert!(!wakeup.requires_broad_rename_scan());

        let mut wakeup = NotifyWakeup::default();
        wakeup.mark_all();
        assert!(wakeup.requires_broad_rename_scan());

        let mut wakeup = NotifyWakeup::default();
        wakeup.add_removed_paths(
            (0..=NOTIFY_RENAME_PATH_LIMIT)
                .map(|i| PathBuf::from(format!("/var/log/archive/{i}.log"))),
        );
        assert!(wakeup.requires_broad_rename_scan());

        let mut wakeup = NotifyWakeup::default();
        wakeup.add_removed_paths([PathBuf::from("/var/log/app.log.1")]);
        assert!(
            wakeup.requires_broad_rename_scan(),
            "raw rename paths are candidates only; an already-removed source path still needs a tree scan"
        );
    }

    #[test]
    fn pathless_wakeup_remains_coarse_after_named_event() {
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths(std::iter::empty());
        wakeup.add_paths([PathBuf::from("/var/log/app.log")]);

        assert!(matches!(&wakeup.state, NotifyWakeupState::All));
        assert!(wakeup.requires_broad_rename_scan());
        assert!(wakeup.names(&PathBuf::from("/var/log/other.log"), None));
    }

    #[test]
    fn incomplete_rename_paths_temporarily_defer_idle_reaping() {
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_removed_paths(
            (0..=NOTIFY_RENAME_PATH_LIMIT)
                .map(|i| PathBuf::from(format!("/var/log/archive/{i}.log"))),
        );

        assert!(wakeup.rename_paths_incomplete());
        assert!(!should_reap_unfindable_watcher(
            true,
            false,
            Duration::from_secs(60),
            Duration::from_secs(5),
            Duration::from_secs(3600),
            wakeup.rename_paths_incomplete(),
        ));
    }

    #[test]
    fn incomplete_rename_recovery_window_is_bounded_and_not_extended() {
        let started_at = time::Instant::now();
        let interval = Duration::from_secs(5);
        let deadline = update_rename_recovery_deadline(None, true, started_at, interval)
            .expect("an incomplete rename burst must start a recovery window");

        let unchanged = update_rename_recovery_deadline(
            Some(deadline),
            false,
            started_at + Duration::from_secs(1),
            interval,
        );
        assert_eq!(
            unchanged,
            Some(deadline),
            "an unrelated wakeup must not extend the recovery window"
        );

        let unchanged_for_same_burst = update_rename_recovery_deadline(
            Some(deadline),
            true,
            started_at + Duration::from_secs(1),
            interval,
        );
        assert_eq!(
            unchanged_for_same_burst,
            Some(deadline),
            "revisiting the same incomplete burst must not extend its recovery window"
        );

        let refreshed = update_rename_recovery_deadline(
            Some(deadline),
            true,
            deadline + Duration::from_millis(1),
            interval,
        )
        .expect("a later incomplete burst may start its own recovery window");
        assert!(
            refreshed > deadline,
            "a new incomplete burst after expiry must get a fresh bounded window"
        );

        assert!(!should_reap_unfindable_watcher(
            true,
            false,
            Duration::from_secs(60),
            interval,
            Duration::from_secs(3600),
            true,
        ));
        assert!(should_reap_unfindable_watcher(
            true,
            false,
            Duration::from_secs(60),
            interval,
            Duration::from_secs(3600),
            false,
        ));
    }

    #[test]
    fn notify_wakeup_take_resets_to_none() {
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([PathBuf::from("/var/log/a.log")]);

        let taken = wakeup.take();
        assert!(taken.is_pending());
        assert!(
            !wakeup.is_pending(),
            "take() must reset the original to None"
        );
    }

    #[test]
    fn idle_unfindable_watcher_survives_one_discovery_interval() {
        // Regression test for a bug found in review: an `Idle` watcher whose file's rename target
        // isn't fingerprint-matched back to it in the very same `discover()` pass that saw it
        // disappear (a slow/partial rename, or a notify event that simply hasn't been delivered
        // yet) must still get a chance to be rediscovered on a later pass, rather than having its
        // checkpoint dropped on the very first pass that finds it unfindable.
        let discovery_interval = Duration::from_secs(5);
        let rotate_wait = Duration::from_secs(3600);

        assert!(
            !should_reap_unfindable_watcher(
                true,
                false,
                Duration::from_millis(1),
                discovery_interval,
                rotate_wait,
                false,
            ),
            "an idle watcher must not be reaped the instant it's first seen unfindable"
        );
        assert!(
            !should_reap_unfindable_watcher(
                true,
                false,
                discovery_interval - Duration::from_millis(1),
                discovery_interval,
                rotate_wait,
                false,
            ),
            "an idle watcher must survive at least one full discovery interval unfindable"
        );
        assert!(
            should_reap_unfindable_watcher(
                true,
                false,
                discovery_interval + Duration::from_millis(1),
                discovery_interval,
                rotate_wait,
                false,
            ),
            "an idle watcher must be reaped once it's been unfindable longer than a discovery \
             interval, rather than waiting out the (possibly effectively-infinite) rotate_wait"
        );
    }

    #[test]
    fn active_unfindable_watcher_keeps_its_rotate_wait_grace_period() {
        // The pre-existing behavior for `Active` watchers (which keep getting read, and on EOF
        // marked dead, every cycle regardless of this check) must be unchanged: only `rotate_wait`
        // governs reaping for them, not `discovery_interval`.
        let discovery_interval = Duration::from_secs(5);
        let rotate_wait = Duration::from_secs(3600);

        assert!(
            !should_reap_unfindable_watcher(
                false,
                false,
                discovery_interval + Duration::from_secs(1),
                discovery_interval,
                rotate_wait,
                false,
            ),
            "an active watcher must not be reaped just because a discovery interval elapsed"
        );
        assert!(
            should_reap_unfindable_watcher(
                false,
                false,
                rotate_wait + Duration::from_millis(1),
                discovery_interval,
                rotate_wait,
                false,
            ),
            "an active watcher must still be reaped once rotate_wait elapses"
        );
    }

    #[test]
    fn idle_watcher_found_outside_glob_keeps_its_rotate_wait_grace_period() {
        let discovery_interval = Duration::from_secs(5);
        let rotate_wait = Duration::from_secs(3600);

        assert!(!should_reap_unfindable_watcher(
            true,
            true,
            rotate_wait - Duration::from_millis(1),
            discovery_interval,
            rotate_wait,
            false,
        ));
        assert!(should_reap_unfindable_watcher(
            true,
            true,
            rotate_wait + Duration::from_millis(1),
            discovery_interval,
            rotate_wait,
            false,
        ));
    }

    #[tokio::test]
    async fn deleted_outside_glob_watcher_returns_to_finite_reaping() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        let archive = directory.path().join("app.log.1");
        std::fs::write(&path, b"line\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        assert!(watcher.read_line().await.unwrap().raw_line.is_some());
        assert!(watcher.read_line().await.unwrap().raw_line.is_none());
        watcher.deactivate().await;

        std::fs::rename(&path, &archive).unwrap();
        watcher.set_file_findable(false);
        watcher.update_path(archive.clone()).await.unwrap();
        watcher.mark_path_outside_glob();
        assert!(watcher.path_is_outside_glob());

        std::fs::remove_file(&archive).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;

        let file_server = FileServer {
            paths_provider,
            max_read_bytes: 1024 * 1024,
            ignore_checkpoints: true,
            read_from: ReadFrom::Beginning,
            ignore_before: None,
            max_line_bytes: 1024,
            line_delimiter: Bytes::from_static(b"\n"),
            data_dir: directory.path().to_path_buf(),
            glob_minimum_cooldown: Duration::from_secs(1),
            fingerprinter: Fingerprinter::new(
                file_source_common::FingerprintStrategy::FirstLinesChecksum {
                    ignored_header_bytes: 0,
                    lines: 1,
                },
                1024,
                true,
            ),
            oldest_first: false,
            remove_after: None,
            emitter: NoopEmitter,
            rotate_wait: Duration::from_secs(3600),
            discovery_mode: FileDiscoveryMode::PollingOnly,
            reconcile_interval: Duration::from_secs(1),
            idle_timeout: Some(Duration::from_secs(60)),
        };
        let mut fp_map = IndexMap::from([(FileFingerprint::DevInode(0, 0), watcher)]);
        let mut lines = Vec::new();
        file_server
            .poll_idle_watchers(&mut fp_map, &mut lines, &NotifyWakeup::default())
            .await;

        let watcher = fp_map.values().next().unwrap();
        assert!(
            !watcher.path_is_outside_glob(),
            "a deleted outside-glob path must no longer use the rotate-wait grace"
        );
        assert!(should_reap_unfindable_watcher(
            true,
            watcher.path_is_outside_glob(),
            watcher.unfindable_for(),
            Duration::from_millis(1),
            Duration::from_secs(3600),
            false,
        ));
    }

    #[test]
    fn notify_wakeup_matches_relative_include_path_once_absolutized() {
        // Regression test for a bug found in review: a notify event names an absolute path (as
        // `notify` always reports), while the glob-discovered path for the same file is relative
        // (as `Glob::paths()` yields for a relative `include` pattern). Without absolutizing the
        // glob path first, `names()` would report `false` even though both sides refer to the
        // same file.
        let cwd = PathBuf::from("/home/user/project");
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([PathBuf::from("/home/user/project/logs/app.log")]);

        let glob_discovered_path = PathBuf::from("logs/app.log");
        assert!(
            !wakeup.names(&glob_discovered_path, None),
            "sanity check: comparing the raw relative path against the absolute notify path \
             must not match"
        );

        let absolutized = crate::absolutize(&glob_discovered_path, Some(&cwd));
        assert!(
            wakeup.names(&absolutized, None),
            "after absolutizing the glob-discovered relative path the same way notify resolves \
             its own watch paths, it must match the notify-reported absolute path"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn notify_wakeup_matches_canonical_file_path() {
        let directory = tempfile::tempdir().unwrap();
        let real_path = directory.path().join("real.log");
        let alias_path = directory.path().join("alias.log");
        std::fs::write(&real_path, b"line\n").unwrap();
        std::os::unix::fs::symlink(&real_path, &alias_path).unwrap();

        let canonical_path = std::fs::canonicalize(&real_path).unwrap();
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([alias_path]);
        wakeup.resolve_canonical_paths().await;

        assert!(
            wakeup.names(&canonical_path, Some(&canonical_path)),
            "a canonical notify path must match a watcher reached through a symlink alias"
        );
    }

    /// Regression test for a bug found in review: the full pass repositioned a rewritten file's
    /// reader *before* `rekey_watcher` checked whether the new fingerprint was already owned. When it
    /// was, the rekey was refused but the reader had already been rewound onto the rewritten content
    /// while still filed under the old key -- it would emit those lines under the wrong identity, and
    /// the full pass could hand the same fingerprint to the other file, duplicating them.
    ///
    /// Asserted as the ordering invariant the fix establishes: the occupancy test is a pure map
    /// lookup, so it must be answerable before any reader is touched. Driving this through
    /// `discover` cannot reach the branch -- a new fingerprint that is already tracked takes the
    /// "already tracked" path instead, and the collision only arises when the other watcher acquires
    /// that key later in the same pass.
    #[tokio::test]
    async fn an_occupied_fingerprint_is_detected_before_any_reader_is_touched() {
        let directory = tempfile::tempdir().unwrap();
        let owner = directory.path().join("owner.log");
        let rewritten = directory.path().join("rewritten.log");
        std::fs::write(&owner, b"owner\n").unwrap();
        std::fs::write(&rewritten, b"rewritten\n").unwrap();

        let occupied_key = FileFingerprint::DevInode(1, 1);
        let stale_key = FileFingerprint::DevInode(2, 2);
        let make = |path: PathBuf| async move {
            FileWatcher::new(
                path,
                ReadFrom::Beginning,
                None,
                1024,
                Bytes::from_static(b"\n"),
                true,
            )
            .await
            .unwrap()
        };
        let mut watcher = make(rewritten.clone()).await;
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let offset_before = watcher.get_file_position();
        assert!(offset_before > 0, "test setup needs a consumed reader");

        let mut fp_map = IndexMap::from([(occupied_key, make(owner).await), (stale_key, watcher)]);

        // The guard the fix adds, evaluated against the same state the full pass would see.
        let collision_is_known_upfront =
            stale_key != occupied_key && fp_map.contains_key(&occupied_key);
        assert!(
            collision_is_known_upfront,
            "the collision must be decidable from the map alone, before any I/O"
        );

        // With the collision known, no reader is repositioned and the refusal is what
        // `rekey_watcher` independently reports.
        assert!(
            !rekey_watcher(
                &mut fp_map,
                &CheckpointsView::default(),
                stale_key,
                occupied_key
            ),
            "rekeying onto an occupied fingerprint must be refused"
        );
        assert_eq!(
            fp_map
                .get(&stale_key)
                .expect("the refused watcher stays under its old key")
                .get_file_position(),
            offset_before,
            "the refused watcher's reader must be exactly where it was"
        );
    }

    /// Regression test for a bug found in review: the symlinked-root check required the symlink to
    /// sit in the include pattern's leading *literal* prefix. An include like `*/**/*.log` has the
    /// prefix `.`, which canonicalizes to itself, so a wildcard component traversing a symlink was
    /// never recognised. A backend reporting physical paths (FSEvents) then matched no spelling and
    /// no root, and the event was dropped as accounted for -- leaving the file unread until the
    /// reconciliation backstop.
    #[cfg(unix)]
    #[tokio::test]
    async fn targeted_pass_defers_for_a_symlink_under_a_wildcard_glob_component() {
        let directory = tempfile::tempdir().unwrap();
        // The physical tree the symlink points into, deliberately outside the include root.
        let physical = directory.path().join("physical");
        std::fs::create_dir(&physical).unwrap();
        let physical_file = physical.join("app.log");
        std::fs::write(&physical_file, b"line\n").unwrap();

        // The include root, whose *wildcard* component is the symlink.
        let include_root = directory.path().join("include");
        std::fs::create_dir(&include_root).unwrap();
        std::os::unix::fs::symlink(&physical, include_root.join("linked")).unwrap();

        // `<include>/*/*.log`: the leading literal prefix is `<include>`, which is a real directory
        // and canonicalizes to itself, so the old prefix-only test found no symlink.
        let pattern = include_root.join("*").join("*.log");
        let paths_provider = crate::paths_provider::Glob::new(
            &[pattern],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        // What FSEvents would report: the physical path, matching neither spelling of the include.
        let mut fp_map = IndexMap::new();
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let accounted_for = file_server
            .discover_changed_paths(
                &HashSet::from([physical_file.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &CheckpointsView::default(),
            )
            .await;

        assert!(
            !accounted_for,
            "a physical path reached through a symlinked wildcard component must defer to a full \
             glob pass, not be discarded as accounted for"
        );
    }

    /// Regression test for a bug found in review: the child-link search read only the literal root's
    /// immediate entries, so a recursive `**` include whose symlink sits below a *real* subdirectory
    /// was missed -- the event was discarded as accounted for and the file waited for the backstop.
    #[cfg(unix)]
    #[tokio::test]
    async fn targeted_pass_defers_for_a_symlink_nested_under_a_recursive_glob() {
        let directory = tempfile::tempdir().unwrap();
        let physical = directory.path().join("physical");
        std::fs::create_dir(&physical).unwrap();
        let physical_file = physical.join("app.log");
        std::fs::write(&physical_file, b"line\n").unwrap();

        // The link sits two levels below the literal root, under a genuine directory.
        let include_root = directory.path().join("include");
        let real_subdirectory = include_root.join("service");
        std::fs::create_dir_all(&real_subdirectory).unwrap();
        std::os::unix::fs::symlink(&physical, real_subdirectory.join("linked")).unwrap();

        let pattern = include_root.join("**").join("*.log");
        let paths_provider = crate::paths_provider::Glob::new(
            &[pattern],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let mut fp_map = IndexMap::new();
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let accounted_for = file_server
            .discover_changed_paths(
                &HashSet::from([physical_file]),
                &mut fp_map,
                &mut known_small_files,
                &CheckpointsView::default(),
            )
            .await;

        assert!(
            !accounted_for,
            "a physical path reached through a symlink nested under a recursive wildcard must \
             defer to a full glob pass"
        );
    }

    /// The looser symlinked-root test must not make every unrelated file force a full pass: that
    /// sweep is the O(N)-per-event cost the targeted pass exists to avoid.
    #[tokio::test]
    async fn targeted_pass_still_discards_an_excluded_sibling_without_a_full_pass() {
        let directory = tempfile::tempdir().unwrap();
        let included = directory.path().join("app.log");
        std::fs::write(&included, b"line\n").unwrap();
        // A sibling in the same watched directory that the include pattern does not match.
        let excluded = directory.path().join("notes.txt");
        std::fs::write(&excluded, b"text\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let mut fp_map = IndexMap::new();
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let accounted_for = file_server
            .discover_changed_paths(
                &HashSet::from([excluded]),
                &mut fp_map,
                &mut known_small_files,
                &CheckpointsView::default(),
            )
            .await;

        assert!(
            accounted_for,
            "an excluded sibling must be settled by spelling alone, without forcing a full pass"
        );
    }

    #[tokio::test]
    async fn notify_wakeup_canonicalizes_only_new_paths_and_retries_missing_paths() {
        let directory = tempfile::tempdir().unwrap();
        let existing_path = directory.path().join("existing.log");
        let missing_path = directory.path().join("missing.log");
        std::fs::write(&existing_path, b"line\n").unwrap();

        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([existing_path.clone()]);
        wakeup.resolve_canonical_paths().await;
        assert!(
            wakeup.canonical_paths_pending.is_empty(),
            "a successfully canonicalized path should leave the pending delta"
        );

        wakeup.add_paths([existing_path.clone()]);
        assert!(
            wakeup.canonical_paths_pending.is_empty(),
            "repeated events for a resolved path must not enqueue another canonicalization"
        );

        wakeup.add_paths([missing_path.clone()]);
        wakeup.resolve_canonical_paths().await;
        assert!(
            wakeup.canonical_paths_pending.is_empty(),
            "a failed canonicalization must leave the per-event pending delta"
        );
        assert!(
            wakeup.canonical_paths_retry.contains(&missing_path),
            "a path that disappeared during a rename must remain retryable"
        );

        std::fs::write(&missing_path, b"line\n").unwrap();
        wakeup.retry_canonical_paths().await;
        assert!(!wakeup.canonical_paths_retry.contains(&missing_path));
        assert!(
            wakeup
                .canonical_paths
                .contains(&std::fs::canonicalize(&missing_path).unwrap())
        );
    }

    #[derive(Clone)]
    struct NoopEmitter;

    impl file_source_common::FileSourceInternalEvents for NoopEmitter {
        fn emit_file_added(&self, _path: &Path) {}
        fn emit_file_resumed(&self, _path: &Path, _file_position: u64) {}
        fn emit_file_watch_error(&self, _path: &Path, _error: std::io::Error) {}
        fn emit_file_unwatched(&self, _path: &Path, _reached_eof: bool) {}
        fn emit_file_deleted(&self, _path: &Path) {}
        fn emit_file_delete_error(&self, _path: &Path, _error: std::io::Error) {}
        fn emit_file_fingerprint_read_error(&self, _path: &Path, _error: std::io::Error) {}
        fn emit_file_checkpointed(&self, _count: usize, _duration: Duration) {}
        fn emit_file_checksum_failed(&self, _path: &Path) {}
        fn emit_file_checkpoint_write_error(&self, _error: std::io::Error) {}
        fn emit_files_open(&self, _count: usize) {}
        fn emit_files_idle(&self, _count: usize) {}
        fn emit_path_globbing_failed(&self, _path: &Path, _error: &std::io::Error) {}
        fn emit_file_line_too_long(&self, _buf: &bytes::BytesMut, _max_size: usize, _size: usize) {}
    }

    fn watcher_position_after(
        fp_map: &IndexMap<FileFingerprint, FileWatcher>,
        key: FileFingerprint,
    ) -> Option<u64> {
        fp_map.get(&key).map(FileWatcher::get_file_position)
    }

    fn test_file_server(
        paths_provider: crate::paths_provider::Glob<NoopEmitter>,
        data_dir: PathBuf,
    ) -> FileServer<crate::paths_provider::Glob<NoopEmitter>, NoopEmitter> {
        FileServer {
            paths_provider,
            max_read_bytes: 1024 * 1024,
            ignore_checkpoints: true,
            read_from: ReadFrom::Beginning,
            ignore_before: None,
            max_line_bytes: 1024,
            line_delimiter: Bytes::from_static(b"\n"),
            data_dir,
            glob_minimum_cooldown: Duration::from_secs(1),
            fingerprinter: Fingerprinter::new(
                file_source_common::FingerprintStrategy::FirstLinesChecksum {
                    ignored_header_bytes: 0,
                    lines: 1,
                },
                1024,
                true,
            ),
            oldest_first: false,
            remove_after: None,
            emitter: NoopEmitter,
            rotate_wait: Duration::from_secs(3600),
            discovery_mode: FileDiscoveryMode::PollingOnly,
            reconcile_interval: Duration::from_secs(1),
            idle_timeout: Some(Duration::from_secs(60)),
        }
    }

    /// Regression test for a review finding: rotation replaces the file at a tracked path, and
    /// reopening there abandoned the inode the reader still held -- losing records it had not read
    /// yet, and the unterminated one in its buffer.
    #[tokio::test]
    async fn a_replaced_path_drains_the_old_inode_before_repointing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let file_id = file_server
            .fingerprinter
            .clone()
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the file must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let mut fp_map = IndexMap::from([(file_id, watcher)]);

        // Written after the reader caught up, then rotated away before discovery runs. These bytes
        // exist only on the old inode.
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"stranded\n"))
            .unwrap();
        std::fs::rename(&path, directory.path().join("app.log.1")).unwrap();
        std::fs::write(&path, b"replacement\n").unwrap();

        let checkpoints = CheckpointsView::default();
        let mut lines = Vec::new();
        let _ = file_server
            .discover(
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
                None,
                &NotifyWakeup::default(),
                &mut lines,
            )
            .await;

        let texts: Vec<_> = lines
            .iter()
            .map(|line| String::from_utf8_lossy(&line.text).into_owned())
            .collect();
        assert!(
            texts.iter().any(|text| text == "stranded"),
            "the rotated-away inode must be drained before the reader moves, got {texts:?}"
        );
        // The pass marks watchers unfindable before discovery, and reaching EOF while unfindable is
        // how `read_line` recognises a deleted file -- so draining must not retire the watcher that
        // was just pointed at the replacement.
        assert!(
            fp_map.values().all(|watcher| !watcher.dead()),
            "the repointed watcher must survive the drain, or the replacement is never read"
        );
    }

    /// A replacement can remain too short for fingerprinting for several passes. That must not
    /// take the active reader's old descriptor away before the fingerprint becomes available.
    #[tokio::test]
    async fn a_short_replacement_drains_the_old_inode_before_repointing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        let archive = directory.path().join("app.log.1");
        std::fs::write(&path, b"header\nold baseline\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.fingerprinter = Fingerprinter::new(
            file_source_common::FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 2,
            },
            1024,
            true,
        );

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let old_file_id = file_server
            .fingerprinter
            .clone()
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the original must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let mut fp_map = IndexMap::from([(old_file_id, watcher)]);

        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"stranded\n"))
            .unwrap();
        std::fs::rename(&path, &archive).unwrap();
        // One line is insufficient for the two-line fingerprint strategy, so the replacement is
        // discovered before its fingerprint is complete.
        std::fs::write(&path, b"new header\n").unwrap();

        let checkpoints = CheckpointsView::default();
        let mut lines = Vec::new();
        let _ = file_server
            .discover(
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
                None,
                &NotifyWakeup::default(),
                &mut lines,
            )
            .await;

        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_ref())
                .collect::<Vec<_>>(),
            vec![&b"stranded"[..]],
            "the old inode must be drained even while the replacement is too short to fingerprint"
        );
        let watcher = fp_map
            .get(&old_file_id)
            .expect("the watcher remains tracked until the replacement fingerprints");
        assert!(
            watcher.path_has_tracked_identity().await,
            "the reader must be repointed only after the old inode was drained"
        );
    }

    #[tokio::test]
    async fn a_same_fingerprint_replacement_also_drains_before_repointing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        let archive = directory.path().join("app.log.1");
        std::fs::write(&path, b"header\nold tail\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let file_id = file_server
            .fingerprinter
            .clone()
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the original must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let mut fp_map = IndexMap::from([(file_id, watcher)]);

        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"stranded\n"))
            .unwrap();
        std::fs::rename(&path, &archive).unwrap();
        // The first fingerprint line intentionally remains the same, so the replacement keeps the
        // old fingerprint and reaches the same-key branch in `discover`.
        std::fs::write(&path, b"header\nnew tail\n").unwrap();

        let checkpoints = CheckpointsView::default();
        let mut lines = Vec::new();
        let _ = file_server
            .discover(
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
                None,
                &NotifyWakeup::default(),
                &mut lines,
            )
            .await;

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, b"stranded"[..]);
        let watcher = fp_map
            .get_mut(&file_id)
            .expect("the watcher remains tracked");
        assert!(watcher.path_has_tracked_identity().await);
        assert_eq!(
            watcher
                .read_line()
                .await
                .unwrap()
                .raw_line
                .expect("the replacement must be read from its beginning")
                .bytes,
            "header"
        );
    }

    /// Regression test for the GitHub review finding: a large rotated tail must be drained in
    /// bounded batches rather than accumulated in the discovery pass's shared `lines` vector.
    #[tokio::test]
    async fn a_rotation_drain_is_bounded_and_resumable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        let archive = directory.path().join("app.log.1");
        std::fs::write(&path, b"first\n").unwrap();

        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        assert!(watcher.read_line().await.unwrap().raw_line.is_some());
        assert!(watcher.read_line().await.unwrap().raw_line.is_none());

        // Both records are on the inode that is about to be rotated away. The first bounded pass
        // must leave the second one on the descriptor so it can be emitted by the next pass.
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"stranded-one\nok\n"))
            .unwrap();
        std::fs::rename(&path, &archive).unwrap();
        std::fs::write(&path, b"replacement\n").unwrap();

        let old_file_id = FileFingerprint::DevInode(0, 0);
        let mut lines = Vec::new();
        let first = drain_and_repoint(&mut watcher, old_file_id, path.clone(), &mut lines, 8)
            .await
            .unwrap();

        assert!(
            matches!(first, DrainOutcome::LimitReached { bytes_read } if bytes_read > 8),
            "the first pass must stop after its byte budget, got {first:?}"
        );
        assert_eq!(
            lines.len(),
            1,
            "one bounded batch should contain one record"
        );
        assert!(
            !watcher.path_has_tracked_identity().await,
            "the watcher must remain attached to the old inode while its tail is pending"
        );

        let second = drain_and_repoint(&mut watcher, old_file_id, path, &mut lines, 8)
            .await
            .unwrap();

        assert!(
            matches!(second, DrainOutcome::Repointed { .. }),
            "the next pass must finish the old inode and repoint"
        );
        assert_eq!(
            lines.len(),
            2,
            "the old inode's complete tail must be preserved"
        );
        assert!(watcher.path_has_tracked_identity().await);
    }

    /// The discovery caller must not rekey a watcher whose bounded drain stopped early: the
    /// next pass has to find the same old reader and continue from its current offset.
    #[tokio::test]
    async fn a_pending_rotation_drain_keeps_the_old_fingerprint() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        let archive = directory.path().join("app.log.1");
        std::fs::write(&path, b"first\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.max_read_bytes = 8;

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let old_file_id = file_server
            .fingerprinter
            .clone()
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the file must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let mut fp_map = IndexMap::from([(old_file_id, watcher)]);

        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"stranded-one\nok\n"))
            .unwrap();
        std::fs::rename(&path, &archive).unwrap();
        std::fs::write(&path, b"replacement\n").unwrap();

        let replacement_file_id = file_server
            .fingerprinter
            .clone()
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the replacement must fingerprint");
        assert_ne!(old_file_id, replacement_file_id);

        let checkpoints = CheckpointsView::default();
        let mut lines = Vec::new();
        let first = file_server
            .discover(
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
                None,
                &NotifyWakeup::default(),
                &mut lines,
            )
            .await;

        assert!(first.drain_pending);
        assert_eq!(lines.len(), 1);
        assert!(fp_map.contains_key(&old_file_id));
        assert!(!fp_map.contains_key(&replacement_file_id));

        lines.clear();
        let second = file_server
            .discover(
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
                None,
                &NotifyWakeup::default(),
                &mut lines,
            )
            .await;

        assert!(!second.drain_pending);
        assert_eq!(lines.len(), 1);
        assert!(!fp_map.contains_key(&old_file_id));
        assert!(fp_map.contains_key(&replacement_file_id));
        assert_eq!(lines[0].text, &b"ok"[..]);
    }

    /// Regression test for a review finding: draining marks the watcher found so reaching EOF is not
    /// read as a deletion, but a failed reopen left it that way -- still "found" on an inode it no
    /// longer describes, where `remove_after` would unlink whatever now occupies its old path.
    #[tokio::test]
    async fn a_failed_repoint_leaves_the_watcher_unfindable() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\n").unwrap();

        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        watcher.prepare_for_discovery();

        // The replacement is gone by the time the reopen runs, as it would be if it were removed in
        // the window after fingerprinting it.
        let mut lines = Vec::new();
        let missing = directory.path().join("vanished.log");
        let repointed = drain_and_repoint(
            &mut watcher,
            FileFingerprint::DevInode(0, 0),
            missing,
            &mut lines,
            1024,
        )
        .await;

        assert!(
            repointed.is_err(),
            "the reopen must fail for this to mean anything"
        );
        assert!(
            !watcher.file_findable(),
            "a failed repoint must leave the watcher unfindable, as the pass left it"
        );
    }

    /// Regression test for a review finding: a file rotated out of the include patterns is
    /// recovered by identity, and `remove_after` then unlinked it -- deleting an archive the
    /// configuration never selected.
    #[tokio::test]
    async fn remove_after_does_not_delete_a_path_outside_the_glob() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("app.log.1");
        std::fs::write(&archive, b"rotated\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.remove_after = Some(Duration::ZERO);

        let mut watcher = FileWatcher::new(
            archive.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        watcher.deactivate().await;
        // Recovered by identity at a path no include pattern matches.
        watcher.mark_path_outside_glob();

        let mut fp_map = IndexMap::from([(FileFingerprint::DevInode(0, 0), watcher)]);
        let mut lines = Vec::new();
        file_server
            .poll_idle_watchers(&mut fp_map, &mut lines, &NotifyWakeup::default())
            .await;

        assert!(
            archive.exists(),
            "remove_after must not unlink a path outside the include patterns"
        );
        assert!(
            fp_map.values().all(|watcher| watcher.dead()),
            "the watcher must still retire, or the path is reconsidered forever"
        );
    }

    /// A timer-only removal pass must revalidate the file first, because a delayed or dropped
    /// notify event can leave new data on disk after the last idle poll.
    #[tokio::test]
    async fn idle_removal_rechecks_for_new_data_before_deleting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.remove_after = Some(Duration::ZERO);

        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        watcher.deactivate().await;
        assert!(watcher.is_idle());
        let file_id = FileFingerprint::DevInode(0, 0);
        let mut fp_map = IndexMap::from([(file_id, watcher)]);

        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"second\n"))
            .unwrap();

        let mut lines = Vec::new();
        file_server
            .remove_idle_watchers_due(&mut fp_map, &mut lines)
            .await;

        assert!(path.exists(), "new data must cancel timer-only removal");
        assert!(lines.is_empty());
        let watcher = fp_map.get_mut(&file_id).expect("watcher is retained");
        assert!(!watcher.dead());
        assert!(
            watcher.is_active(),
            "the changed idle watcher must reactivate"
        );
        assert_eq!(
            watcher
                .read_line()
                .await
                .unwrap()
                .raw_line
                .expect("the appended line must remain readable")
                .bytes,
            b"second"[..]
        );
    }

    /// A failed metadata/removal attempt must not leave an expired idle watcher making the main
    /// loop's sleep duration zero on every iteration.
    #[tokio::test]
    async fn failed_idle_removal_is_backed_off() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.remove_after = Some(Duration::ZERO);

        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        watcher.deactivate().await;
        std::fs::remove_file(&path).unwrap();

        let file_id = FileFingerprint::DevInode(0, 0);
        let mut fp_map = IndexMap::from([(file_id, watcher)]);
        let mut lines = Vec::new();
        file_server
            .remove_idle_watchers_due(&mut fp_map, &mut lines)
            .await;

        assert!(
            file_server
                .next_idle_removal_delay(&fp_map)
                .is_some_and(|delay| delay > Duration::ZERO),
            "a failed idle removal must make the main loop sleep before retrying"
        );
    }

    /// Regression test for a review finding: `reconcile_interval_secs` takes any `u64`, and a value
    /// large enough to mean "effectively never" must not reuse the already-due startup deadline.
    #[test]
    fn an_unrepresentable_interval_schedules_a_future_deadline() {
        let now = time::Instant::now();
        let current = now;

        assert_eq!(
            schedule_after(now, Duration::from_secs(5), current),
            now + Duration::from_secs(5),
            "an ordinary interval schedules normally"
        );
        assert!(
            schedule_after(now, Duration::from_secs(u64::MAX), current) > now,
            "an interval the clock cannot represent must still schedule in the future"
        );
    }

    /// Regression test for a review finding: a wakeup naming one path still statted every idle
    /// watcher, so one busy writer swept thousands of files on every event -- the per-file-per-pass
    /// cost this mode exists to remove.
    #[tokio::test]
    async fn a_named_wakeup_does_not_poll_the_files_it_does_not_name() {
        let directory = tempfile::tempdir().unwrap();
        let named = directory.path().join("busy.log");
        let unnamed = directory.path().join("quiet.log");
        std::fs::write(&named, b"first\n").unwrap();
        std::fs::write(&unnamed, b"first\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let mut fp_map = IndexMap::new();
        for (index, path) in [&named, &unnamed].into_iter().enumerate() {
            let mut watcher = FileWatcher::new(
                path.clone(),
                ReadFrom::Beginning,
                None,
                1024,
                Bytes::from_static(b"\n"),
                true,
            )
            .await
            .unwrap();
            while watcher.read_line().await.unwrap().raw_line.is_some() {}
            watcher.deactivate().await;
            assert!(watcher.is_idle(), "test setup requires idle watchers");
            fp_map.insert(FileFingerprint::DevInode(0, index as u64), watcher);
        }

        // Both files grow, but the wakeup names only one of them.
        for path in [&named, &unnamed] {
            std::fs::OpenOptions::new()
                .append(true)
                .open(path)
                .and_then(|mut file| std::io::Write::write_all(&mut file, b"second\n"))
                .unwrap();
        }
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([named.clone()]);

        let mut lines = Vec::new();
        file_server
            .poll_idle_watchers(&mut fp_map, &mut lines, &wakeup)
            .await;

        let reactivated = |path: &Path| {
            fp_map
                .values()
                .find(|watcher| watcher.path == path)
                .expect("watcher must still be tracked")
                .is_active()
        };
        assert!(
            reactivated(&named),
            "the named file must be polled and resumed"
        );
        assert!(
            !reactivated(&unnamed),
            "a file the wakeup did not name must not be statted, let alone resumed"
        );
    }

    #[tokio::test]
    async fn idle_watcher_survives_a_temporary_short_fingerprint_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"complete\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        assert!(watcher.read_line().await.unwrap().raw_line.is_some());
        assert!(watcher.read_line().await.unwrap().raw_line.is_none());
        watcher.deactivate().await;
        assert!(watcher.is_idle());

        // A writer can be observed between writing the bytes and writing their delimiter. The
        // fingerprint fails with UnexpectedEof, but the existing idle watcher must remain alive.
        std::fs::write(&path, b"partial").unwrap();
        let mut fp_map = IndexMap::from([(FileFingerprint::DevInode(0, 0), watcher)]);
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let checkpoints = CheckpointsView::default();
        assert!(
            file_server
                .discover(
                    &mut fp_map,
                    &mut known_small_files,
                    &checkpoints,
                    None,
                    &NotifyWakeup::default(),
                    &mut Vec::new(),
                )
                .await
                .keep_notify_discovery
        );

        let mut lines = Vec::new();
        file_server
            .poll_idle_watchers(&mut fp_map, &mut lines, &NotifyWakeup::default())
            .await;
        let watcher = fp_map.values_mut().next().unwrap();
        assert!(
            watcher.is_active(),
            "the idle watcher must not be reaped on a short read"
        );

        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"\n").unwrap();
        file.flush().unwrap();

        let line = watcher
            .read_line()
            .await
            .unwrap()
            .raw_line
            .expect("the partial line should be completed after its delimiter arrives");
        assert_eq!(line.bytes.as_ref(), b"partial");
    }

    #[tokio::test]
    async fn full_pass_rekeys_rather_than_duplicating_a_rewritten_watcher() {
        // Regression test for a bug found in review: the full pass looked a file up only by its
        // *current* fingerprint. After an in-place rewrite changed the hashed prefix, the existing
        // watcher stayed under the old key, the new fingerprint looked untracked, and
        // `watch_new_file` added a second reader for the same file while the first stayed alive --
        // duplicating every line.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"original\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let original_key = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &path,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("app.log must fingerprint");
        let watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        let mut fp_map = IndexMap::from([(original_key, watcher)]);

        // Rewrite in place with a different first line: same path, new fingerprint.
        std::fs::write(&path, b"rewritten\n").unwrap();
        let new_key = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &path,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("the rewritten file must fingerprint");
        assert_ne!(original_key, new_key);

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let checkpoints = CheckpointsView::default();
        assert!(
            file_server
                .discover(
                    &mut fp_map,
                    &mut known_small_files,
                    &checkpoints,
                    None,
                    &NotifyWakeup::default(),
                    &mut Vec::new(),
                )
                .await
                .keep_notify_discovery
        );

        assert_eq!(
            fp_map.len(),
            1,
            "the rewritten file must keep exactly one watcher, not gain a duplicate reader"
        );
        assert!(
            fp_map.contains_key(&new_key),
            "the surviving watcher must be keyed by the fingerprint the file now has"
        );
    }

    #[tokio::test]
    async fn full_pass_keeps_an_active_watcher_findable_during_a_partial_rewrite() {
        // Regression test for a bug found in review: the full pass marks every watcher unfindable up
        // front, and its fingerprint-failure handling only rescued *idle* watchers. An active file
        // rewritten in place, observed before its new prefix hashes, therefore stayed
        // `findable == false` -- which `read_line` reads as "deleted" -- and a later completed
        // fingerprint started a second watcher on the same path, duplicating records.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"original line\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &path,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("app.log must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        assert!(watcher.is_active(), "test setup requires an active watcher");
        let mut fp_map = IndexMap::from([(file_id, watcher)]);

        // Truncate and rewrite with no complete line yet: fingerprinting fails with UnexpectedEof.
        std::fs::write(&path, b"partial").unwrap();
        assert_eq!(
            file_server
                .fingerprinter
                .fingerprint_or_emit(
                    &path,
                    &mut file_source_common::KnownSmallFiles::default(),
                    &NoopEmitter
                )
                .await,
            None,
            "test setup requires the rewritten prefix to be unfingerprintable"
        );

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let checkpoints = CheckpointsView::default();
        assert!(
            file_server
                .discover(
                    &mut fp_map,
                    &mut known_small_files,
                    &checkpoints,
                    None,
                    &NotifyWakeup::default(),
                    &mut Vec::new(),
                )
                .await
                .keep_notify_discovery
        );

        assert_eq!(fp_map.len(), 1, "no second watcher may be created");
        let watcher = fp_map.values().next().unwrap();
        assert!(
            watcher.file_findable(),
            "an active watcher whose rewrite has not hashed yet must stay findable, or read_line              treats the file as deleted"
        );
        assert_eq!(
            watcher.get_file_position(),
            0,
            "the reader must have been reset, not left inside the discarded content"
        );
    }

    #[tokio::test]
    async fn duplicate_fingerprint_defers_an_untracked_path_to_the_full_pass() {
        // Regression test for a bug found in review: `FirstLinesChecksum` gives two files with the
        // same first line one `FileFingerprint`, and only one of them can occupy that key. An event
        // for the *other* file used to resolve the fingerprint, find the first file's watcher, and
        // silently do nothing -- the path lookup sat in an `else if` a fingerprint hit skipped.
        //
        // The map state here is the one the server can actually reach: a single `Fingerprinter` with
        // one strategy means every key is of the same kind, so the colliding second file is simply
        // not tracked. It must therefore be handed to the full glob pass, which is what discovers
        // and inserts it -- not silently dropped.
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.log");
        let second = directory.path().join("second.log");
        // Identical fingerprinted prefix (one line), so both files fingerprint the same.
        std::fs::write(&first, b"shared first line\n").unwrap();
        std::fs::write(&second, b"shared first line\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let shared_id = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &first,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("first.log must fingerprint");
        assert_eq!(
            file_server
                .fingerprinter
                .fingerprint_or_emit(
                    &second,
                    &mut file_source_common::KnownSmallFiles::default(),
                    &NoopEmitter
                )
                .await,
            Some(shared_id),
            "test setup requires both files to share one fingerprint"
        );

        // Only `first.log` is tracked; it owns the shared key.
        let first_watcher = FileWatcher::new(
            first.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        let mut fp_map = IndexMap::from([(shared_id, first_watcher)]);
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let checkpoints = CheckpointsView::default();

        let accounted = file_server
            .discover_changed_paths(
                &HashSet::from([second.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;

        assert!(
            !accounted,
            "an event for a file that collides on the fingerprint but is not tracked must demand              the full glob pass, not be silently dropped"
        );
        assert_eq!(
            fp_map.len(),
            1,
            "the targeted pass must not attach the event to the colliding watcher"
        );
        assert_eq!(
            fp_map.values().next().unwrap().path,
            first,
            "the tracked watcher must still be the one it was"
        );
    }

    #[tokio::test]
    async fn targeted_pass_defers_a_vanished_active_path_to_the_full_pass() {
        // Regression test for a bug found in review (and explicitly noted as untested): a queued
        // modification event can be processed after its file is gone. `fingerprint_or_emit` then
        // returns `None` and `update_path` fails, but the branch still called `mark_found` and
        // reported the path accounted for -- skipping the full pass and leaving the active watcher
        // attached to the old inode until the backstop.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &path,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("app.log must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        assert!(watcher.is_active(), "the watcher must still be active");

        // The file disappears before the queued event is processed.
        std::fs::remove_file(&path).unwrap();

        let mut fp_map = IndexMap::from([(file_id, watcher)]);
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        assert!(
            !file_server
                .discover_changed_paths(
                    &HashSet::from([path.clone()]),
                    &mut fp_map,
                    &mut known_small_files,
                    &CheckpointsView::default(),
                )
                .await,
            "a vanished path must demand the full pass rather than be marked found"
        );
    }

    #[tokio::test]
    async fn targeted_pass_rekeys_an_in_place_rewrite() {
        // Regression test for a bug found in review: an in-place rewrite (`copytruncate`, or an app
        // rewriting its own log) changes the first line `FirstLinesChecksum` hashes. The watcher
        // stayed under its old `fp_map` key, so emitted lines were checkpointed under an identity the
        // file no longer had, and the next full pass did not recognise the new fingerprint as tracked
        // and started a second watcher on the same path.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"original first\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let original_key = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &path,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("app.log must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let mut fp_map = IndexMap::from([(original_key, watcher)]);

        let checkpoints = CheckpointsView::default();
        checkpoints.register(original_key, 15, fp_map[&original_key].generation());

        // Rewrite in place with a different first line: same path, new fingerprint.
        std::fs::write(&path, b"rewritten first\n").unwrap();
        let new_key = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &path,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("the rewritten file must fingerprint");
        assert_ne!(
            original_key, new_key,
            "test setup requires the rewrite to change the fingerprint"
        );

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        file_server
            .discover_changed_paths(
                &HashSet::from([path.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;

        assert!(
            fp_map.contains_key(&new_key),
            "the watcher must now be keyed by the fingerprint the file actually has"
        );
        assert!(
            !fp_map.contains_key(&original_key),
            "the stale key must be gone, or the full pass adds a second watcher for this path"
        );
        assert_eq!(
            fp_map.len(),
            1,
            "exactly one watcher must remain for the one file"
        );
        assert_eq!(
            checkpoints.get(new_key),
            Some(0),
            "an in-place rewrite restarts the reader, so the checkpoint must follow it to zero --              persisting the pre-rewrite offset would make a restart skip the rewritten content"
        );
        assert_eq!(
            watcher_position_after(&fp_map, new_key),
            Some(0),
            "the reader itself must have restarted"
        );
    }

    /// Regression test for a bug found in review: the second-rewrite check lived inside
    /// `restart_after_rewrite`, but every discovery branch tested `rewind_pending()` first and
    /// skipped the call, so it never ran in production. Driving `discover_changed_paths` is the
    /// point of this test -- calling the watcher directly passes either way.
    #[tokio::test]
    async fn a_second_rewrite_before_the_first_fingerprints_rewinds_through_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\nsecond\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.fingerprinter = Fingerprinter::new(
            file_source_common::FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 2,
            },
            1024,
            true,
        );

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the two-line file must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let mut fp_map = IndexMap::from([(file_id, watcher)]);
        let checkpoints = CheckpointsView::default();

        // First rewrite, still too short to fingerprint: the reader is rewound onto it and reads it.
        std::fs::write(&path, b"alpha\n").unwrap();
        file_server
            .discover_changed_paths(
                &HashSet::from([path.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;
        let watcher = fp_map.values_mut().next().unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        assert!(
            watcher.get_file_position() > 0,
            "the first rewrite must have been read"
        );

        // A second rewrite lands before the first ever fingerprinted: different content, still too
        // short, and deliberately LONGER than the first so no size comparison can see it. The
        // prefix not continuing the one rewound for is the only remaining signal.
        std::fs::write(&path, b"beta-is-longer\n").unwrap();
        file_server
            .discover_changed_paths(
                &HashSet::from([path.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;

        let watcher = fp_map.values_mut().next().unwrap();
        assert_eq!(
            watcher.get_file_position(),
            0,
            "a second rewrite must reposition the reader, or its content is spliced onto the first"
        );
        let line = watcher.read_line().await.unwrap().raw_line;
        assert_eq!(
            line.map(|line| line.bytes),
            Some(Bytes::from_static(b"beta-is-longer")),
            "the reader must be on the newest rewrite"
        );
    }

    /// Regression test for a bug found in review: a *second* rewrite that arrives already complete
    /// looks like the first one having grown -- both are "the fingerprint succeeded under a new
    /// key". Only the prefix separates them, so the completed outcome must carry one too.
    #[tokio::test]
    async fn a_second_rewrite_that_completes_rewinds_rather_than_splicing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\nsecond\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.fingerprinter = Fingerprinter::new(
            file_source_common::FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 2,
            },
            1024,
            true,
        );

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the two-line file must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let mut fp_map = IndexMap::from([(file_id, watcher)]);
        let checkpoints = CheckpointsView::default();

        // First rewrite, too short to fingerprint: the reader is rewound onto it and reads it.
        std::fs::write(&path, b"alpha\n").unwrap();
        file_server
            .discover_changed_paths(
                &HashSet::from([path.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;
        let watcher = fp_map.values_mut().next().unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        assert!(watcher.get_file_position() > 0, "the first rewrite is read");

        // A different rewrite, arriving already long enough to fingerprint. Had `alpha\n` simply
        // grown a second line the reader would be correctly positioned, so the distinction is
        // exactly that this content does not begin with what was rewound for.
        std::fs::write(&path, b"gamma\ndelta\n").unwrap();
        file_server
            .discover_changed_paths(
                &HashSet::from([path.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;

        let watcher = fp_map.values_mut().next().unwrap();
        assert_eq!(
            watcher.get_file_position(),
            0,
            "a different rewrite completing must reposition the reader, not splice onto the old tail"
        );
    }

    #[tokio::test]
    async fn a_completed_rewrite_does_not_rewind_a_reader_that_already_restarted() {
        // The reader is rewound while the rewrite is still incomplete, reads from it, and only then
        // does the fingerprint complete. Rewinding again there replays what it emitted.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\nsecond\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.fingerprinter = Fingerprinter::new(
            file_source_common::FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 2,
            },
            1024,
            true,
        );

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the two-line file must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let mut fp_map = IndexMap::from([(file_id, watcher)]);
        let checkpoints = CheckpointsView::default();

        // Rewritten to one line: incomplete, so the reader is rewound.
        std::fs::write(&path, b"new\n").unwrap();
        file_server
            .discover_changed_paths(
                &HashSet::from([path.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;
        let watcher = fp_map.values_mut().next().unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let position_after_reading = watcher.get_file_position();
        assert!(
            position_after_reading > 0,
            "the rewritten line must be read"
        );

        // The author completes the second line: the fingerprint now succeeds under a new key.
        std::fs::write(&path, b"new\nmore\n").unwrap();
        file_server
            .discover_changed_paths(
                &HashSet::from([path.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;

        let watcher = fp_map.values_mut().next().unwrap();
        assert!(
            watcher.get_file_position() >= position_after_reading,
            "completing the rewrite rewound the reader again, replaying the line it already emitted"
        );
    }

    #[tokio::test]
    async fn a_full_pass_reopens_an_active_watcher_whose_path_was_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"old\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        // Two lines required, so the short replacement below cannot fingerprint.
        file_server.fingerprinter = Fingerprinter::new(
            file_source_common::FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 2,
            },
            1024,
            true,
        );

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        std::fs::write(&path, b"old\nsecond\n").unwrap();
        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the original must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        assert!(watcher.is_active(), "test setup requires an active watcher");
        let mut fp_map = IndexMap::from([(file_id, watcher)]);

        // Atomically replaced by a *different* inode holding a too-short prefix.
        let replacement = directory.path().join("replacement.tmp");
        std::fs::write(&replacement, b"brand new\n").unwrap();
        std::fs::rename(&replacement, &path).unwrap();

        let _ = file_server
            .discover(
                &mut fp_map,
                &mut known_small_files,
                &CheckpointsView::default(),
                None,
                &NotifyWakeup::default(),
                &mut Vec::new(),
            )
            .await;

        let watcher = fp_map.values_mut().next().unwrap();
        assert!(
            watcher.path_has_tracked_identity().await,
            "the watcher must have been reopened onto the replacement, not left on the old inode"
        );
    }

    /// A replacement can be yielded before the rotated archive in one glob pass. The replacement
    /// takes the old watcher after its descriptor is drained, while the archive is then a new path
    /// under the old fingerprint. It must resume at the drained offset rather than replaying the
    /// tail that was already emitted during the handoff.
    #[tokio::test]
    async fn a_rotated_archive_resumes_after_the_drain_without_duplicate_lines() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        let archive = directory.path().join("app.log.1");
        std::fs::write(&path, b"old-header\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log*")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.ignore_checkpoints = false;

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let old_file_id = file_server
            .fingerprinter
            .clone()
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the original must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let old_position = watcher.get_file_position();
        let old_generation = watcher.generation();
        let mut fp_map = IndexMap::from([(old_file_id, watcher)]);
        let checkpoints = CheckpointsView::default();
        checkpoints.register(old_file_id, old_position, old_generation);

        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, b"stranded\n"))
            .unwrap();
        std::fs::rename(&path, &archive).unwrap();
        std::fs::write(&path, b"new-header\n").unwrap();
        let replacement_file_id = file_server
            .fingerprinter
            .clone()
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the replacement must fingerprint");
        assert_ne!(old_file_id, replacement_file_id);

        let mut lines = Vec::new();
        let _ = file_server
            .discover(
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
                None,
                &NotifyWakeup::default(),
                &mut lines,
            )
            .await;

        let stranded: Vec<_> = lines
            .iter()
            .filter(|line| line.text == b"stranded"[..])
            .collect();
        assert_eq!(
            stranded.len(),
            1,
            "the old inode's tail must be emitted exactly once"
        );
        assert!(fp_map.contains_key(&replacement_file_id));
        let archive_watcher = fp_map
            .get(&old_file_id)
            .expect("the archive must get its own watcher");
        let archive_position = archive_watcher.get_file_position();
        assert_eq!(
            archive_position,
            std::fs::metadata(&archive).unwrap().len(),
            "the archive watcher must resume after the drained tail"
        );
        assert_eq!(
            checkpoints.get_acknowledged(old_file_id),
            Some(old_position),
            "the unacknowledged drain must not be persisted as already delivered"
        );
        checkpoints.update(old_file_id, archive_position, old_generation);
        assert_eq!(
            checkpoints.get_acknowledged(old_file_id),
            Some(archive_position),
            "the drained generation's acknowledgement must advance the durable position"
        );
    }

    #[tokio::test]
    async fn a_repeatedly_failing_fingerprint_restarts_the_reader_only_once() {
        // A file rewritten to fewer lines than `FirstLinesChecksum` needs fails to fingerprint on
        // *every* discovery pass. Restarting the reader each time re-emits what it already consumed.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\nsecond\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        file_server.fingerprinter = Fingerprinter::new(
            file_source_common::FingerprintStrategy::FirstLinesChecksum {
                ignored_header_bytes: 0,
                lines: 2,
            },
            1024,
            true,
        );

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the two-line file must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let mut fp_map = IndexMap::from([(file_id, watcher)]);
        let checkpoints = CheckpointsView::default();

        std::fs::write(&path, b"new\n").unwrap();

        let _ = file_server
            .discover(
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
                None,
                &NotifyWakeup::default(),
                &mut Vec::new(),
            )
            .await;
        let watcher = fp_map.values_mut().next().unwrap();
        let mut lines = Vec::new();
        while let Ok(RawLineResult {
            raw_line: Some(line),
            ..
        }) = watcher.read_line().await
        {
            lines.push(line.bytes);
        }
        assert_eq!(lines.len(), 1, "the rewritten line is read once: {lines:?}");
        let position_after_reading = watcher.get_file_position();

        // Later passes must leave the reader where it is; the fingerprint still fails.
        for pass in 0..3 {
            let _ = file_server
                .discover(
                    &mut fp_map,
                    &mut known_small_files,
                    &checkpoints,
                    None,
                    &NotifyWakeup::default(),
                    &mut Vec::new(),
                )
                .await;
            let watcher = fp_map.values_mut().next().unwrap();
            assert_eq!(
                watcher.get_file_position(),
                position_after_reading,
                "pass {pass} rewound the reader, so the line it already emitted is emitted again"
            );
        }
    }

    #[tokio::test]
    async fn targeted_pass_restarts_the_reader_after_an_in_place_rewrite() {
        // Regression test for a bug found in review: rekeying moved the map key and checkpoint but
        // left the reader at its old offset with its old buffer. After a `copytruncate`-style rewrite
        // the reader would seek past EOF -- losing everything until the file grew past the old
        // offset -- or splice the rewritten bytes onto the tail of the discarded content.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"old one\nold two\nold three\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let original_key = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &path,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("app.log must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        // Read the original content so the reader sits at a non-zero offset.
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        assert!(
            watcher.get_file_position() > 0,
            "test setup requires a non-zero read offset"
        );
        let mut fp_map = IndexMap::from([(original_key, watcher)]);

        // Truncate and rewrite in place: same inode, new first line, and *shorter* than the old
        // offset -- the case where resuming would seek past EOF and lose the rewrite entirely.
        std::fs::write(&path, b"new\n").unwrap();

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let checkpoints = CheckpointsView::default();
        file_server
            .discover_changed_paths(
                &HashSet::from([path.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;

        let watcher = fp_map.values_mut().next().expect("one watcher remains");
        assert_eq!(
            watcher.get_file_position(),
            0,
            "the reader must start the rewritten file over"
        );
        assert_eq!(
            watcher
                .read_line()
                .await
                .unwrap()
                .raw_line
                .expect("the rewritten content must be readable")
                .bytes,
            "new",
            "the rewritten content must be read, not skipped past"
        );
    }

    #[tokio::test]
    async fn rekey_watcher_moves_the_watcher_and_its_checkpoint() {
        // `update_key` has existed on the checkpointer since #5215 (2020) but had no caller after
        // its one-off checksum migration was removed. Rekeying needs exactly it: the `fp_map` key is
        // what `Line::file_id` carries downstream and what the checkpointer persists under.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"line\n").unwrap();

        let old_key = FileFingerprint::DevInode(1, 1);
        let new_key = FileFingerprint::DevInode(2, 2);
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        // Read so the position is non-zero: that marks this as a file that was *appended* to, whose
        // reader stays where it is -- as opposed to one restarted by an in-place rewrite, covered by
        // `rekey_watcher_zeroes_the_checkpoint_for_a_restarted_reader`.
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        assert!(watcher.get_file_position() > 0);
        let mut fp_map = IndexMap::from([(old_key, watcher)]);

        let checkpoints = CheckpointsView::default();
        checkpoints.register(old_key, 42, fp_map[&old_key].generation());

        assert!(rekey_watcher(&mut fp_map, &checkpoints, old_key, new_key));
        assert!(
            fp_map.contains_key(&new_key),
            "the watcher must be reachable under its new identity"
        );
        assert!(
            !fp_map.contains_key(&old_key),
            "the stale key must not keep a second entry for the same file"
        );
        assert_eq!(
            checkpoints.get(new_key),
            Some(42),
            "an appended-to file keeps its reader position, so its checkpoint must follow unchanged"
        );
        assert_eq!(
            checkpoints.get(old_key),
            None,
            "the stale checkpoint must not linger under the old identity"
        );
    }

    #[tokio::test]
    async fn targeted_pass_does_not_restart_a_gzip_reader_on_a_size_comparison() {
        // Regression test for a bug found in review: the rewrite check compares the reader's
        // position with the file's on-disk size, but for a gzip watcher the position counts
        // *decoded* bytes while the size is *compressed* bytes. Any ordinary compressible file then
        // looks truncated, and restarting it decodes from byte zero and re-emits every record
        // already sent.
        use async_compression::tokio::bufread::GzipEncoder;
        use tokio::io::AsyncReadExt as _;

        async fn encode(data: &[u8]) -> Vec<u8> {
            let mut out = Vec::new();
            GzipEncoder::new(data).read_to_end(&mut out).await.unwrap();
            out
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log.gz");
        // Highly compressible: decoded length far exceeds the compressed size on disk, which is
        // exactly the shape that tripped the comparison.
        let decoded = "a".repeat(4096) + "\n";
        std::fs::write(&path, encode(decoded.as_bytes()).await).unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.gz")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &path,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("the gzip file must fingerprint");
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            8192,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        assert!(watcher.is_gzip(), "test setup requires a gzip watcher");
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        let position_before = watcher.get_file_position();
        assert!(
            position_before > std::fs::metadata(&path).unwrap().len(),
            "test setup requires the decoded position to exceed the compressed size"
        );

        let mut fp_map = IndexMap::from([(file_id, watcher)]);
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let checkpoints = CheckpointsView::default();
        file_server
            .discover_changed_paths(
                &HashSet::from([path.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &checkpoints,
            )
            .await;

        let watcher = fp_map.values().next().expect("the watcher remains");
        assert_eq!(
            watcher.get_file_position(),
            position_before,
            "a gzip watcher must not be restarted by a decoded-vs-compressed size comparison"
        );
    }

    #[tokio::test]
    async fn rekey_watcher_keeps_its_position_for_oldest_first() {
        // Regression test for a bug found in review: `fp_map`'s order is read priority under
        // `oldest_first`, and `shift_remove` + `insert` moved a rekeyed watcher to the tail -- so a
        // newer file drained before an older one that happened to be rewritten.
        let directory = tempfile::tempdir().unwrap();
        let make = |name: &str| {
            let path = directory.path().join(name);
            std::fs::write(&path, b"x\n").unwrap();
            async move {
                FileWatcher::new(
                    path,
                    ReadFrom::Beginning,
                    None,
                    1024,
                    Bytes::from_static(b"\n"),
                    true,
                )
                .await
                .unwrap()
            }
        };
        let oldest = FileFingerprint::DevInode(1, 1);
        let middle = FileFingerprint::DevInode(2, 2);
        let newest = FileFingerprint::DevInode(3, 3);
        let mut fp_map = IndexMap::from([
            (oldest, make("a.log").await),
            (middle, make("b.log").await),
            (newest, make("c.log").await),
        ]);

        let rekeyed = FileFingerprint::DevInode(9, 9);
        let checkpoints = CheckpointsView::default();
        assert!(rekey_watcher(&mut fp_map, &checkpoints, middle, rekeyed));

        let order: Vec<FileFingerprint> = fp_map.keys().copied().collect();
        assert_eq!(
            order,
            vec![oldest, rekeyed, newest],
            "a rekeyed watcher must keep its position, not move to the tail"
        );
    }

    /// Regression test for a bug found in review: `watch_new_file` inserted a watcher without
    /// registering it as the owner of its fingerprint, so `update` refused every checkpoint it
    /// produced and a restart reread the whole file.
    #[tokio::test]
    async fn a_newly_watched_file_owns_its_checkpoint() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"first\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let file_id = file_server
            .fingerprinter
            .clone()
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the file must fingerprint");

        let mut fp_map = IndexMap::new();
        let checkpoints = CheckpointsView::default();
        file_server
            .watch_new_file(path.clone(), file_id, &mut fp_map, &checkpoints, false)
            .await;

        let watcher = fp_map.get(&file_id).expect("the watcher must be installed");
        checkpoints.update(file_id, 6, watcher.generation());
        assert_eq!(
            checkpoints.get(file_id),
            Some(6),
            "a newly watched file must be able to record its progress"
        );
    }

    /// Regression test for a bug found in review: a reset to zero under an *unchanged* fingerprint
    /// left the generation alone, so an acknowledgement sent before the reset still matched and
    /// wound the checkpoint back past it -- resuming a restart inside content already discarded.
    #[test]
    fn a_reset_under_the_same_fingerprint_retires_the_previous_generation() {
        let checkpoints = CheckpointsView::default();
        let file_id = FileFingerprint::FirstLinesChecksum(1);

        let before_reset = file_source_common::next_owner_generation();
        checkpoints.register(file_id, 500, before_reset);

        // The reader is repositioned onto new content under the same fingerprint, as a truncation
        // or a same-name replacement does.
        let after_reset = file_source_common::next_owner_generation();
        checkpoints.register(file_id, 0, after_reset);

        // An acknowledgement for a line read before the reset.
        checkpoints.update(file_id, 500, before_reset);
        assert_eq!(
            checkpoints.get(file_id),
            Some(0),
            "an acknowledgement from before the reset must not wind the checkpoint back"
        );

        checkpoints.update(file_id, 12, after_reset);
        assert_eq!(
            checkpoints.get(file_id),
            Some(12),
            "the reader that owns the new content records normally"
        );
    }

    #[tokio::test]
    async fn rekey_watcher_zeroes_the_checkpoint_for_a_restarted_reader() {
        // Regression test for a bug found in review: after an in-place rewrite the reader restarts at
        // zero, but `update_key` carried the *pre-rewrite* offset onto the new fingerprint. A restart
        // before new content arrived would then resume at that stale offset and skip the beginning of
        // the rewritten file.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"line\n").unwrap();

        let old_key = FileFingerprint::DevInode(1, 1);
        let new_key = FileFingerprint::DevInode(2, 2);
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        // A *genuine* restart, not merely a zero offset: an unread watcher also sits at zero, and
        // `rekey_watcher` must carry its resumed checkpoint over rather than discarding it.
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        std::fs::write(&path, b"rewritten\n").unwrap();
        watcher
            .restart_after_rewrite()
            .await
            .expect("restart must succeed");
        assert_eq!(watcher.get_file_position(), 0);
        let mut fp_map = IndexMap::from([(old_key, watcher)]);

        let checkpoints = CheckpointsView::default();
        checkpoints.register(old_key, 42, fp_map[&old_key].generation());

        assert!(rekey_watcher(&mut fp_map, &checkpoints, old_key, new_key));
        assert_eq!(
            checkpoints.get(new_key),
            Some(0),
            "a restarted reader must not leave a pre-rewrite offset persisted"
        );
    }

    #[tokio::test]
    async fn rekey_watcher_refuses_to_evict_a_colliding_owner() {
        // Two files can share one `FirstLinesChecksum` (identical first lines). Rekeying onto an
        // occupied key would evict the watcher that legitimately owns it, losing its reader and its
        // position, so the collision must be refused and left to the full pass.
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.log");
        let second = directory.path().join("second.log");
        std::fs::write(&first, b"a\n").unwrap();
        std::fs::write(&second, b"b\n").unwrap();

        let stale_key = FileFingerprint::DevInode(1, 1);
        let occupied_key = FileFingerprint::DevInode(2, 2);
        let make = |path: PathBuf| async move {
            FileWatcher::new(
                path,
                ReadFrom::Beginning,
                None,
                1024,
                Bytes::from_static(b"\n"),
                true,
            )
            .await
            .unwrap()
        };
        let mut fp_map = IndexMap::from([
            (stale_key, make(first.clone()).await),
            (occupied_key, make(second.clone()).await),
        ]);
        let checkpoints = CheckpointsView::default();

        assert!(
            !rekey_watcher(&mut fp_map, &checkpoints, stale_key, occupied_key),
            "rekeying onto an occupied fingerprint must be refused"
        );
        assert_eq!(
            fp_map
                .get(&occupied_key)
                .map(|watcher| watcher.path.clone()),
            Some(second),
            "the colliding owner must keep its entry"
        );
        assert!(
            fp_map.contains_key(&stale_key),
            "the refused watcher must be left where it was, not dropped"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn targeted_pass_records_the_configured_spelling_for_a_short_file() {
        // A backend like FSEvents reports a symlinked include's canonical target. Fingerprinting under
        // that spelling would make it the one `remove_after` may unlink -- a file outside the include,
        // possibly shared -- so the watcher's configured path is used instead.
        let directory = tempfile::tempdir().unwrap();
        let target_dir = directory.path().join("physical");
        std::fs::create_dir(&target_dir).unwrap();
        let target = target_dir.join("app.log");
        std::fs::write(&target, b"complete\n").unwrap();

        let link_dir = directory.path().join("logs");
        std::os::unix::fs::symlink(&target_dir, &link_dir).unwrap();
        let configured = link_dir.join("app.log");

        let paths_provider = crate::paths_provider::Glob::new(
            &[link_dir.join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        // Tracked under the configured (symlinked) spelling.
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(&configured, &mut known_small_files, &NoopEmitter)
            .await
            .expect("the complete file must fingerprint");
        let watcher = FileWatcher::new(
            configured.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        let mut fp_map = IndexMap::from([(file_id, watcher)]);

        // Rewritten to an unterminated record, so it now lands in `known_small_files`.
        std::fs::write(&target, b"partial").unwrap();

        // The event names the *canonical target*, as FSEvents would.
        file_server
            .discover_changed_paths(
                &HashSet::from([target.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &CheckpointsView::default(),
            )
            .await;

        let canonical_identity = target.canonicalize().unwrap();
        if let Some(removal_path) = known_small_files.removal_path(&canonical_identity) {
            assert_ne!(
                removal_path,
                canonical_identity.as_path(),
                "remove_after must never be handed the canonical target of a symlinked include"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn targeted_pass_ignores_paths_outside_the_include_rules() {
        // Regression test for a bug found in review: notify watches whole directories, so events
        // name files the include/exclude rules leave out. Fingerprinting such a path put it into
        // `known_small_files` when short or unterminated, and `remove_after` then deleted a file the
        // user had explicitly excluded.
        let directory = tempfile::tempdir().unwrap();
        let included = directory.path().join("app.log");
        let excluded = directory.path().join("secret.txt");
        std::fs::write(&included, b"line\n").unwrap();
        // No trailing delimiter: this is exactly the shape that lands in `known_small_files`.
        std::fs::write(&excluded, b"short").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let mut fp_map = IndexMap::new();
        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        file_server
            .discover_changed_paths(
                &HashSet::from([excluded.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &CheckpointsView::default(),
            )
            .await;

        assert!(
            known_small_files.is_empty(),
            "an excluded path must never enter known_small_files, or remove_after deletes it: {known_small_files:?}"
        );

        // The included file, by contrast, is still processed normally.
        std::fs::write(&included, b"partial").unwrap();
        file_server
            .discover_changed_paths(
                &HashSet::from([included.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &CheckpointsView::default(),
            )
            .await;
        assert_eq!(
            known_small_files.len(),
            1,
            "an included short file must still be tracked"
        );
    }

    #[tokio::test]
    async fn targeted_pass_checks_each_batch_path_against_its_own_canonical_form() {
        // Regression test for a bug found in review: a debounced batch carries several paths at once.
        // Matching a watcher by asking "is this path anywhere in the batch" (`NotifyWakeup::names`)
        // made any watcher match any path in the batch, so an untracked path B riding along with a
        // tracked path A was treated as accounted for and the full glob pass was skipped -- leaving
        // B unread until the reconciliation backstop.
        let directory = tempfile::tempdir().unwrap();
        let tracked = directory.path().join("tracked.log");
        let untracked = directory.path().join("untracked.log");
        std::fs::write(&tracked, b"tracked\n").unwrap();
        std::fs::write(&untracked, b"brand new\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let tracked_id = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &tracked,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("tracked.log must fingerprint");
        let watcher = FileWatcher::new(
            tracked.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        let mut fp_map = IndexMap::from([(tracked_id, watcher)]);
        let mut known_small_files = file_source_common::KnownSmallFiles::default();

        // Both paths in one batch: the tracked one must not vouch for the untracked one.
        assert!(
            !file_server
                .discover_changed_paths(
                    &HashSet::from([tracked.clone(), untracked.clone()]),
                    &mut fp_map,
                    &mut known_small_files,
                    &CheckpointsView::default(),
                )
                .await,
            "an untracked path in a multi-path batch must still demand a full glob pass"
        );
    }

    #[tokio::test]
    async fn targeted_pass_defers_to_a_full_pass_for_an_untracked_path() {
        // Regression test for a bug found in review: some notify backends report a brand-new file as
        // `Modify(Data)` with no create event. The targeted pass fingerprinted such a path, found it
        // under neither a tracked fingerprint nor a tracked name, and did nothing -- leaving the file
        // unread until `reconcile_interval` fired. It must instead report that a full glob pass is
        // still owed.
        let directory = tempfile::tempdir().unwrap();
        let tracked = directory.path().join("tracked.log");
        let untracked = directory.path().join("untracked.log");
        std::fs::write(&tracked, b"tracked\n").unwrap();
        std::fs::write(&untracked, b"brand new\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        let tracked_id = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &tracked,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("tracked.log must fingerprint");
        let watcher = FileWatcher::new(
            tracked.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        let mut fp_map = IndexMap::from([(tracked_id, watcher)]);
        let mut known_small_files = file_source_common::KnownSmallFiles::default();

        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([tracked.clone()]);
        wakeup.resolve_canonical_paths().await;
        assert!(
            file_server
                .discover_changed_paths(
                    &HashSet::from([tracked.clone()]),
                    &mut fp_map,
                    &mut known_small_files,
                    &CheckpointsView::default(),
                )
                .await,
            "a tracked path needs no full pass"
        );

        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([untracked.clone()]);
        wakeup.resolve_canonical_paths().await;
        assert!(
            !file_server
                .discover_changed_paths(
                    &HashSet::from([untracked.clone()]),
                    &mut fp_map,
                    &mut known_small_files,
                    &CheckpointsView::default(),
                )
                .await,
            "an untracked path must demand a full glob pass so the new file is picked up"
        );
    }

    #[tokio::test]
    async fn targeted_pass_matches_a_watcher_through_its_canonical_alias() {
        // Regression test for a bug found in review: the targeted notify pass compared only the raw
        // event path, while the full pass also consults the canonical paths `NotifyWakeup` resolved.
        // When a file is tracked under one spelling and notify reports another that canonicalizes to
        // the same file, neither side of `matches_path` agreed and the watcher was never nudged --
        // sustained events on the alias kept taking this targeted path, deferring appended data.
        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("app.log");
        std::fs::write(&real, b"first\n").unwrap();

        // An alias spelling of the same file that `absolutize` alone cannot reconcile with the
        // event path, so only canonical comparison connects the two. A symlink is the shape the bug
        // was reported for; Windows needs elevation to create one, so a UNC-prefixed path -- the
        // same situation of a distinct raw path with an identical canonical form -- is used there.
        #[cfg(unix)]
        let alias = {
            let alias = directory.path().join("alias.log");
            std::os::unix::fs::symlink(&real, &alias).unwrap();
            alias
        };
        #[cfg(windows)]
        let alias = PathBuf::from(format!(
            r"\\?\{}",
            real.to_str().expect("temp path must be valid UTF-8")
        ));
        assert_ne!(alias, real, "the alias must be a distinct raw path");
        assert_eq!(
            std::fs::canonicalize(&alias).unwrap(),
            std::fs::canonicalize(&real).unwrap(),
            "the alias must canonicalize to the same file"
        );

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());

        // The watcher is created on the alias spelling, so `watcher.path` is the alias while the
        // event below names the real path.
        let mut watcher = FileWatcher::new(
            alias.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        while watcher.read_line().await.unwrap().raw_line.is_some() {}
        assert!(
            !watcher.should_read(),
            "test setup requires the watcher to be mid-EOF-backoff"
        );

        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&real)
            .unwrap();
        file.write_all(b"second\n").unwrap();
        file.flush().unwrap();

        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(
                &real,
                &mut file_source_common::KnownSmallFiles::default(),
                &NoopEmitter,
            )
            .await
            .expect("app.log must fingerprint");
        let mut fp_map = IndexMap::from([(file_id, watcher)]);
        let mut known_small_files = file_source_common::KnownSmallFiles::default();

        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([real.clone()]);
        wakeup.resolve_canonical_paths().await;

        file_server
            .discover_changed_paths(
                &HashSet::from([real.clone()]),
                &mut fp_map,
                &mut known_small_files,
                &CheckpointsView::default(),
            )
            .await;

        let watcher = fp_map.values_mut().next().unwrap();
        assert!(
            watcher.should_read(),
            "a watcher held under an alias spelling must be nudged by an event naming the same file"
        );
    }

    #[tokio::test]
    async fn notify_event_reopens_same_name_replacement_with_unchanged_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("app.log");
        std::fs::write(&path, b"old line\nold tail\n").unwrap();

        let paths_provider = crate::paths_provider::Glob::new(
            &[directory.path().join("*.log")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();
        let mut file_server = test_file_server(paths_provider, directory.path().to_path_buf());
        let mut watcher = FileWatcher::new(
            path.clone(),
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
            true,
        )
        .await
        .unwrap();
        assert!(watcher.read_line().await.unwrap().raw_line.is_some());
        assert!(watcher.read_line().await.unwrap().raw_line.is_some());
        assert!(watcher.read_line().await.unwrap().raw_line.is_none());
        watcher.deactivate().await;

        let old_metadata = std::fs::metadata(&path).unwrap();
        let replacement_path = directory.path().join("replacement.tmp");
        // Keep the first fingerprint line, size, and mtime unchanged. Only the inode and the
        // later line differ, which is exactly the collision that size/mtime-only polling misses.
        std::fs::write(&replacement_path, b"old line\nnew tail\n").unwrap();
        let replacement = std::fs::OpenOptions::new()
            .write(true)
            .open(&replacement_path)
            .unwrap();
        replacement
            .set_modified(old_metadata.modified().unwrap())
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::rename(&replacement_path, &path).unwrap();
        let new_metadata = std::fs::metadata(&path).unwrap();
        assert_eq!(old_metadata.len(), new_metadata.len());
        assert_eq!(
            old_metadata.modified().unwrap(),
            new_metadata.modified().unwrap()
        );

        let mut known_small_files = file_source_common::KnownSmallFiles::default();
        let file_id = file_server
            .fingerprinter
            .fingerprint_or_emit(&path, &mut known_small_files, &NoopEmitter)
            .await
            .unwrap();
        let mut fp_map = IndexMap::from([(file_id, watcher)]);
        let checkpoints = CheckpointsView::default();
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([path.clone()]);
        assert!(
            file_server
                .discover(
                    &mut fp_map,
                    &mut known_small_files,
                    &checkpoints,
                    None,
                    &wakeup,
                    &mut Vec::new(),
                )
                .await
                .keep_notify_discovery
        );
        let mut lines = Vec::new();
        file_server
            .poll_idle_watchers(&mut fp_map, &mut lines, &wakeup)
            .await;

        let watcher = fp_map.values_mut().next().unwrap();
        assert!(watcher.is_active());
        let line = watcher
            .read_line()
            .await
            .unwrap()
            .raw_line
            .expect("replacement contents should be read after the notify event");
        assert_eq!(line.bytes.as_ref(), b"old line");
        let line = watcher
            .read_line()
            .await
            .unwrap()
            .raw_line
            .expect("the replacement tail should also be read");
        assert_eq!(line.bytes.as_ref(), b"new tail");
    }

    /// Like `NoopEmitter`, but records the most recent `files_open`/`files_idle` gauge values so
    /// a test can observe whether `FileServer` ever moved a watcher to `Idle`.
    #[derive(Clone)]
    struct OpenIdleCountingEmitter {
        open: Arc<std::sync::atomic::AtomicUsize>,
        idle: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl file_source_common::FileSourceInternalEvents for OpenIdleCountingEmitter {
        fn emit_file_added(&self, _path: &Path) {}
        fn emit_file_resumed(&self, _path: &Path, _file_position: u64) {}
        fn emit_file_watch_error(&self, _path: &Path, _error: std::io::Error) {}
        fn emit_file_unwatched(&self, _path: &Path, _reached_eof: bool) {}
        fn emit_file_deleted(&self, _path: &Path) {}
        fn emit_file_delete_error(&self, _path: &Path, _error: std::io::Error) {}
        fn emit_file_fingerprint_read_error(&self, _path: &Path, _error: std::io::Error) {}
        fn emit_file_checkpointed(&self, _count: usize, _duration: Duration) {}
        fn emit_file_checksum_failed(&self, _path: &Path) {}
        fn emit_file_checkpoint_write_error(&self, _error: std::io::Error) {}
        fn emit_files_open(&self, count: usize) {
            self.open.store(count, std::sync::atomic::Ordering::SeqCst);
        }
        fn emit_files_idle(&self, count: usize) {
            self.idle.store(count, std::sync::atomic::Ordering::SeqCst);
        }
        fn emit_path_globbing_failed(&self, _path: &Path, _error: &std::io::Error) {}
        fn emit_file_line_too_long(&self, _buf: &bytes::BytesMut, _max_size: usize, _size: usize) {}
    }

    /// A `Sink<Vec<Line>>` that just appends everything it's given to a shared `Vec`, for
    /// collecting `FileServer::run`'s output in a test without pulling in a full downstream
    /// pipeline. `futures-util`'s own channel-based sinks aren't guaranteed available here (this
    /// crate depends on `futures-util` with default features disabled), so this is a minimal
    /// hand-rolled implementation instead.
    #[derive(Clone)]
    struct CollectSink(Arc<std::sync::Mutex<Vec<Line>>>);

    impl Sink<Vec<Line>> for CollectSink {
        type Error = std::convert::Infallible;

        fn poll_ready(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn start_send(self: std::pin::Pin<&mut Self>, item: Vec<Line>) -> Result<(), Self::Error> {
            self.0.lock().unwrap().extend(item);
            Ok(())
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn idle_gzip_watcher_resumes_appended_member_after_idle_close() {
        // End-to-end regression test: a gzip watcher reads member 1, goes idle (handle closed)
        // after `idle_timeout`, then member 2 is appended. Reactivation must read only member 2,
        // not lose it and not replay member 1.
        use async_compression::tokio::bufread::GzipEncoder;
        use tokio::io::AsyncReadExt as _;

        use crate::paths_provider::Glob;

        async fn encode(data: &[u8]) -> Vec<u8> {
            let mut out = Vec::new();
            GzipEncoder::new(data).read_to_end(&mut out).await.unwrap();
            out
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("idle_gzip.gz");
        std::fs::write(&path, encode(b"first\n").await).unwrap();

        let paths_provider = Glob::new(
            &[dir.path().join("*.gz")],
            &[],
            glob::MatchOptions::default(),
            NoopEmitter,
        )
        .unwrap();

        let checkpoint_dir = tempfile::tempdir().unwrap();
        let checkpointer =
            file_source_common::checkpointer::Checkpointer::new(checkpoint_dir.path());

        let idle_timeout = Duration::from_millis(50);
        let emitter = OpenIdleCountingEmitter {
            open: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            idle: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        };
        let file_server = FileServer {
            paths_provider,
            max_read_bytes: 1024 * 1024,
            ignore_checkpoints: true,
            read_from: ReadFrom::Beginning,
            ignore_before: None,
            max_line_bytes: 1024,
            line_delimiter: Bytes::from_static(b"\n"),
            data_dir: checkpoint_dir.path().to_path_buf(),
            glob_minimum_cooldown: Duration::from_millis(20),
            fingerprinter: file_source_common::Fingerprinter::new(
                file_source_common::FingerprintStrategy::FirstLinesChecksum {
                    ignored_header_bytes: 0,
                    lines: 1,
                },
                1024,
                true,
            ),
            oldest_first: false,
            remove_after: None,
            emitter: emitter.clone(),
            rotate_wait: Duration::from_secs(3600),
            discovery_mode: FileDiscoveryMode::PollingOnly,
            reconcile_interval: Duration::from_secs(3600),
            idle_timeout: Some(idle_timeout),
        };

        let lines = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = CollectSink(Arc::clone(&lines));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let (checkpointer_shutdown_tx, checkpointer_shutdown_rx) =
            tokio::sync::oneshot::channel::<()>();

        let run_handle = tokio::spawn(file_server.run(
            sink,
            futures::FutureExt::map(shutdown_rx, |_| ()),
            futures::FutureExt::map(checkpointer_shutdown_rx, |_| ()),
            checkpointer,
        ));

        // Wait for the member to be read, confirming the watcher actually started up correctly.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if !lines.lock().unwrap().is_empty() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for the gzip member to be read"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Wait for the watcher to be idle-closed.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if emitter.idle.load(std::sync::atomic::Ordering::SeqCst) > 0 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for the watcher to go idle"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            emitter.open.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "a skipped gzip backlog must not be reported as an open file"
        );

        // Append member 2 after the handle has been closed.
        let mut combined = encode(b"first\n").await;
        combined.extend_from_slice(&encode(b"second\n").await);
        std::fs::write(&path, &combined).unwrap();

        // Only "second" should show up; "first" must not be replayed.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let got: Vec<String> = lines
                .lock()
                .unwrap()
                .iter()
                .map(|l| String::from_utf8_lossy(&l.text).into_owned())
                .collect();
            if got.iter().any(|l| l == "second") {
                assert_eq!(got, vec!["first".to_string(), "second".to_string()]);
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for the appended gzip member to be read; got {got:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        drop(shutdown_tx);
        drop(checkpointer_shutdown_tx);
        run_handle.await.expect("file_server task panicked").ok();
    }
}
