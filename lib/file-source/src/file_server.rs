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
    FileFingerprint, FileSourceInternalEvents, Fingerprinter, ReadFrom,
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
    file_watcher::{FileWatcher, RawLineResult},
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

/// Minimum time between two full glob+fingerprint reconciliation passes (`discover`) triggered by
/// notify events. Without this, a file under sustained writes would trigger a full re-glob on
/// every `NOTIFY_EVENT_DEBOUNCE` window indefinitely. Doesn't delay reads of already-tracked
/// files (those run every main-loop iteration regardless), but does delay
/// `FileWatcher::mark_ready_to_read`'s nudge by up to this much, since that only happens inside
/// `discover`.
const MIN_NOTIFY_DISCOVERY_INTERVAL: Duration = Duration::from_millis(500);

/// Above this many distinct paths accumulated from notify events since the last reconciliation
/// pass, stop tracking them individually and fall back to treating the wakeup as "something
/// changed, go check everything" (`NotifyWakeup::All`). This bounds the memory a burst of events
/// across many different paths can make `NotifyWakeup::Paths` hold onto, and avoids the
/// per-watcher `HashSet` lookups in `discover`'s hot loop becoming worse than just nudging every
/// watcher once the set is large enough that "every watcher" and "every named path" are close in
/// size anyway.
const NOTIFY_WAKEUP_PATH_LIMIT: usize = 1024;

/// Accumulates, between reconciliation passes, which specific paths (if known) notify events have
/// named -- so that `discover`'s "nudge this watcher past its read-pacing timers" step (see
/// `FileWatcher::mark_ready_to_read`) only touches watchers a concrete event actually named,
/// instead of every currently-tracked watcher on every single notify event regardless of which
/// path it was about. The latter is an O(N) cost (N = number of tracked files) per event, which
/// under a large `include` glob turns "one file got appended to" into "redundantly reconsider
/// every other file's read pacing too."
#[derive(Debug, Default)]
enum NotifyWakeup {
    /// No notify event has arrived since the last reconciliation pass.
    #[default]
    None,
    /// One or more notify events arrived, each naming specific paths (`PathsChanged`/
    /// `PathsRemoved`), and the total distinct path count so far has stayed at or under
    /// `NOTIFY_WAKEUP_PATH_LIMIT`.
    Paths(HashSet<PathBuf>),
    /// A notify event arrived that doesn't name specific paths at all (`Overflow`,
    /// `BackendError`), or the accumulated path count exceeded `NOTIFY_WAKEUP_PATH_LIMIT`: treat
    /// every currently-tracked watcher as possibly needing a nudge, same as the pre-existing
    /// coarse "just rerun discovery" behavior.
    All,
}

impl NotifyWakeup {
    fn is_pending(&self) -> bool {
        !matches!(self, NotifyWakeup::None)
    }

    fn add_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        match self {
            NotifyWakeup::All => {}
            NotifyWakeup::None => {
                let set: HashSet<PathBuf> = paths.into_iter().collect();
                *self = if set.len() > NOTIFY_WAKEUP_PATH_LIMIT {
                    NotifyWakeup::All
                } else {
                    NotifyWakeup::Paths(set)
                };
            }
            NotifyWakeup::Paths(existing) => {
                existing.extend(paths);
                if existing.len() > NOTIFY_WAKEUP_PATH_LIMIT {
                    *self = NotifyWakeup::All;
                }
            }
        }
    }

    fn mark_all(&mut self) {
        *self = NotifyWakeup::All;
    }

    fn take(&mut self) -> NotifyWakeup {
        std::mem::take(self)
    }

    /// Whether `path` should have its watcher nudged past its own read-pacing timers (see
    /// `FileWatcher::mark_ready_to_read`) for this reconciliation pass.
    fn names(&self, path: &Path) -> bool {
        match self {
            NotifyWakeup::None => false,
            NotifyWakeup::All => true,
            NotifyWakeup::Paths(paths) => paths.contains(path),
        }
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
/// An `Idle` watcher, by contrast, is never read while unfindable (`poll_idle_watchers` skips
/// unfindable watchers outright, to avoid reactivating against a different file that's since
/// appeared at the same path -- see that function's doc comment), so it has no other path to
/// reaping at all. Waiting out the full `rotate_wait` (whose default is effectively unlimited)
/// before reaping it would let every rotation past an `include` glob permanently add another
/// watcher/checkpoint to `fp_map`. But reaping it the instant it's first seen unfindable is also
/// wrong: a rename's target might not be fingerprint-matched back to this watcher in the exact
/// same `discover()` pass that saw it disappear (a slow/partial rename, or -- under `Notify` mode
/// -- the create/rename-to event simply hasn't been delivered/debounced through yet), in which
/// case it would still be matched on a *later* pass if given the chance. This grants an `Idle`
/// watcher at least one full `discovery_interval` -- the same cadence `discover()` itself already
/// runs on -- to be rediscovered before reaping it: long enough to survive a rename spanning one
/// discovery pass, but nowhere near `rotate_wait`'s effectively-unlimited default.
fn should_reap_unfindable_watcher(
    is_idle: bool,
    unfindable_for: Duration,
    discovery_interval: Duration,
    rotate_wait: Duration,
) -> bool {
    (is_idle && unfindable_for > discovery_interval) || unfindable_for > rotate_wait
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
        start_offset: line.offset,
        end_offset,
    });
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

        let mut known_small_files = HashMap::new();

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
        // using `reconcile_interval` as the poll interval, rather than failing the whole file
        // source.
        let include_patterns = self.paths_provider.watch_roots();
        let mut notify_discovery = match self.discovery_mode {
            FileDiscoveryMode::Notify if !include_patterns.is_empty() => {
                match NotifyDiscovery::new(&include_patterns, &self.emitter) {
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

        // Alright friends, how does this work?
        //
        // We want to avoid burning up users' CPUs. To do this we sleep after
        // reading lines out of files. But! We want to be responsive as well. We
        // keep track of a 'backoff_cap' to decide how long we'll wait in any
        // given loop. This cap grows each time we fail to read lines in an
        // exponential fashion to some hard-coded cap. To reduce time using glob,
        // we do not re-scan for major file changes (new files, moves, deletes),
        // or write new checkpoints, on every iteration.
        //
        // Discovery trigger, discovery_mode == PollingOnly: re-scan on a fixed interval
        // (`glob_minimum_cooldown`), exactly as before.
        //
        // Discovery trigger, discovery_mode == Notify: re-scan is triggered by (a) an OS-level
        // filesystem event arriving (in which case we still run the *same* full glob+fingerprint
        // reconciliation logic below -- we deliberately don't try to interpret notify's event
        // payload and update state incrementally, since that would duplicate/risk diverging from
        // the already-correct reconciliation logic; a full reconcile pass is cheap enough to run
        // on every event since it's no longer gated by a tiny fixed interval), or (b) the much
        // longer `reconcile_interval` backstop timer firing, to catch anything notify missed
        // (queue overflow, pre-watch-establishment changes, or platforms/paths where notify
        // can't watch for some reason).
        let mut next_glob_time = time::Instant::now();
        // The very first loop iteration always runs a discovery pass regardless of discovery
        // mode: `next_glob_time` was just set to `Instant::now()` above, and `now_time` inside the
        // loop is captured strictly later, so `next_glob_time <= now_time` is unconditionally true
        // on that first check -- no separate "force the first pass" flag is needed. This pass must
        // not be treated as notify-triggered (that would wrongly nudge every watcher's read pacing
        // via `NotifyWakeup::All`/`Paths` on startup, and under `PollingOnly` a notify-triggered
        // pass should never happen at all), so `pending_notify_wakeup` starts at `None`.
        let mut pending_notify_wakeup = NotifyWakeup::None;
        // Throttles notify-triggered full reconciliation passes independently of the backstop
        // timer (`next_glob_time`/`discovery_interval`): see `MIN_NOTIFY_DISCOVERY_INTERVAL`'s
        // doc comment for why. Starts at "now" so the very first notify event, whenever it
        // arrives, is handled immediately rather than waiting out this interval from process
        // start for no reason.
        let mut next_notify_discovery_time = time::Instant::now();
        loop {
            // Use `reconcile_interval` whenever `Notify` mode was configured, even if the notify
            // watcher isn't currently live (it failed to initialize, or died mid-run and was set
            // to `None`): `glob_minimum_cooldown` is documented as ignored in `Notify` mode, so a
            // user relying on that must still get `reconcile_interval`'s cadence during a fallback
            // rather than silently reverting to whatever `glob_minimum_cooldown` happens to be set
            // to (which, precisely because it's documented as ignored, may be tuned very
            // differently than the intended discovery cadence).
            let discovery_interval = if self.discovery_mode == FileDiscoveryMode::Notify {
                self.reconcile_interval
            } else {
                self.glob_minimum_cooldown
            };

            // Glob find files to follow, but not too often. A pending notify wakeup only
            // triggers this early (ahead of `next_glob_time`) once `next_notify_discovery_time`
            // has also elapsed -- see `MIN_NOTIFY_DISCOVERY_INTERVAL`.
            let now_time = time::Instant::now();
            let notify_wakeup_ready =
                pending_notify_wakeup.is_pending() && next_notify_discovery_time <= now_time;
            if next_glob_time <= now_time || notify_wakeup_ready {
                // Leave the wakeup queued (don't take it) if we're here only because the backstop
                // timer fired while the notify throttle hasn't elapsed yet.
                let woken_by_notify_event = if notify_wakeup_ready {
                    next_notify_discovery_time =
                        now_time.checked_add(MIN_NOTIFY_DISCOVERY_INTERVAL).unwrap();
                    pending_notify_wakeup.take()
                } else {
                    NotifyWakeup::None
                };
                // Schedule the next backstop reconciliation time.
                next_glob_time = now_time.checked_add(discovery_interval).unwrap();

                if stats.started_at.elapsed() > Duration::from_secs(1) {
                    stats.report();
                }

                if stats.started_at.elapsed() > Duration::from_secs(10) {
                    stats = TimingStats::default();
                }

                let start = time::Instant::now();
                let keep_notify_discovery = self
                    .discover(
                        &mut fp_map,
                        &mut known_small_files,
                        &checkpoints,
                        notify_discovery.as_mut(),
                        &woken_by_notify_event,
                    )
                    .await;
                if !keep_notify_discovery {
                    warn!(
                        "Notify-based discovery unavailable; relying on periodic reconciliation only."
                    );
                    // `NotifyDiscovery::drop` hands the watcher to a detached teardown thread, so
                    // this never runs `notify`'s own (possibly blocking/panicking) `Drop` here.
                    notify_discovery = None;
                }
                stats.record("discovery", start.elapsed());

                let start = time::Instant::now();
                self.poll_idle_watchers(&mut fp_map, &mut lines).await;
                stats.record("idle-poll", start.elapsed());
            }

            // Cleanup the known_small_files
            if let Some(grace_period) = self.remove_after {
                let mut set = JoinSet::new();

                let remove_file_tasks: HashMap<Id, PathBuf> = known_small_files
                    .iter()
                    .filter(|&(_path, last_time_open)| last_time_open.elapsed() >= grace_period)
                    .map(|(path, _last_time_open)| path.clone())
                    .map(|path| {
                        let path_ = path.clone();
                        let abort_handle =
                            set.spawn(async move { (path_.clone(), remove_file(&path_).await) });
                        (abort_handle.id(), path)
                    })
                    .collect();

                while let Some(res) = set.join_next().await {
                    match res {
                        Ok((path, Ok(()))) => {
                            let removed = known_small_files.remove(&path);

                            if removed.is_some() {
                                self.emitter.emit_file_deleted(&path);
                            }
                        }
                        Ok((path, Err(err))) => {
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
            let mut global_bytes_read: usize = 0;
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
                    watcher.last_seen().elapsed(),
                    discovery_interval,
                    self.rotate_wait,
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
                    checkpoints.set_dead(*file_id);
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
            //
            // Capped at `next_notify_discovery_time` when a notify wakeup is already pending:
            // otherwise, once `backoff_cap` has grown large from a quiet spell, a pending wakeup
            // with no further events to cut the sleep short (see the `tokio::select!` below) would
            // wait out the full backoff instead of the much shorter `MIN_NOTIFY_DISCOVERY_INTERVAL`
            // throttle it's actually waiting on. Uses a fresh `Instant::now()`, not the `now_time`
            // captured at the top of the loop: `discover`/reading files/sending downstream can
            // take a while, and computing the remaining time against a stale timestamp would
            // overstate it, adding back some of the latency this cap exists to remove.
            let sleep_duration = if pending_notify_wakeup.is_pending() {
                Duration::from_millis(backoff as u64)
                    .min(next_notify_discovery_time.saturating_duration_since(time::Instant::now()))
            } else {
                Duration::from_millis(backoff as u64)
            };
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
                            || !self.handle_notify_message(msg, discovery, &mut pending_notify_wakeup);
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
                                        || !self.handle_notify_message(msg, discovery, &mut pending_notify_wakeup)
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
    fn handle_notify_message(
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
            Some(NotifyMessage::PathsRemoved(paths)) => {
                trace!(message = "Received file removal notification.", ?paths);
                // If one of the removed paths is itself a directory we're watching (as opposed to
                // a file inside one), the watch on it may have been invalidated at the OS level
                // (this is inotify's behavior on Linux: removing a watched directory invalidates
                // the watch on that inode, even if a new directory is later created at the same
                // path). Forget our bookkeeping for it so the reconciliation pass's
                // `resync_watches` call re-`watch`es it once it exists again, rather than
                // wrongly believing it's still watched and skipping it forever. See
                // `NotifyDiscovery::forget_watch` for details.
                for path in &paths {
                    if discovery.is_watched_dir(path) {
                        discovery.forget_watch(path);
                    }
                }
                pending_notify_wakeup.add_paths(paths);
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
        true
    }

    /// Perform a full glob+fingerprint reconciliation pass: re-glob the configured `include`
    /// patterns and detect new files, renames (a known fingerprint appearing at a new path), and
    /// duplicate-fingerprint conflicts (picking the most recently modified file).
    ///
    /// This is the same logic that used to run unconditionally on every `glob_minimum_cooldown`
    /// tick. It's now called either on a fixed interval (`PollingOnly` mode, or as the
    /// `Notify`-mode backstop via `reconcile_interval`), or on-demand when the OS-level notify
    /// watcher reports a change -- throttled to at most once per `MIN_NOTIFY_DISCOVERY_INTERVAL`
    /// regardless of how often notify events arrive, since sustained writes to even a single file
    /// would otherwise trigger this full pass on every `NOTIFY_EVENT_DEBOUNCE` window indefinitely
    /// (see that constant's doc comment). We deliberately keep this as one unified, full pass
    /// rather than writing a separate "apply this one notify event incrementally" code path:
    /// reusing the already-correct logic avoids a second, potentially divergent implementation of
    /// rename/duplicate-fingerprint handling, and the throttle above keeps its cost bounded
    /// without needing that split.
    ///
    /// `notify_wakeup` distinguishes a pass triggered by an actual OS-level filesystem event from
    /// one triggered by the periodic timer alone (`glob_minimum_cooldown` in `PollingOnly` mode,
    /// or the `reconcile_interval` backstop in `Notify` mode): only for a watcher whose path
    /// `notify_wakeup` actually names (`NotifyWakeup::Paths`) or when it's `NotifyWakeup::All`
    /// (an event that didn't name specific paths, e.g. `Overflow`/`BackendError`, or more distinct
    /// paths than `NOTIFY_WAKEUP_PATH_LIMIT`) does an already-tracked, still-`Active` watcher get
    /// nudged past its own independent read-pacing timers (see the "same path" branch below and
    /// `FileWatcher::mark_ready_to_read`) -- a concrete "this path changed" signal justifies
    /// reading it sooner than those timers would otherwise allow, but the periodic timer firing on
    /// its own doesn't, and nudging every watcher on every pass regardless (the pre-fix behavior)
    /// meant a single notify event under a large `include` glob cost an O(N) sweep of every other
    /// tracked file's read pacing too, not just the one path that actually changed.
    ///
    /// `notify_wakeup.names(&path)` compares paths as reported by the OS notify backend against
    /// `path` as yielded by `paths_provider.paths()`. The `notify` crate always resolves the path
    /// it was asked to `watch()` to an absolute one internally (via the current working directory)
    /// before using it, and reports its events using that same absolute form -- but a glob-based
    /// `PathsProvider` can yield a relative path unchanged if the configured `include` pattern was
    /// itself relative. Without accounting for this, `notify_wakeup.names(&path)` would compare a
    /// relative `path` against an absolute event path and never match, silently defeating the
    /// nudge for every file matched by a relative `include` pattern. `discover` absolutizes `path`
    /// (via `crate::absolutize`) the same way `notify` would before comparing.
    ///
    /// **Known limitation**: this only accounts for relative-vs-absolute, not full
    /// canonicalization (symlink resolution): canonicalizing every tracked file's path on every
    /// pass, just to cover a much rarer case, would cost a `stat`-like syscall per file per pass
    /// for a benefit that's purely about read-latency, not correctness. If an `include` pattern
    /// traverses a symlink and the two sides resolve it differently even after absolutizing, the
    /// nudge can still silently not fire for that watcher on that pass. This degrades gracefully:
    /// `should_read`'s own timers still fire eventually, and the periodic
    /// `reconcile_interval`/`glob_minimum_cooldown` backstop still runs regardless of this nudge,
    /// so the affected file falls back to ordinary polling-like latency rather than losing data or
    /// getting stuck.
    /// Returns `false` if notify-based discovery must be disabled entirely (the watcher failed to
    /// rebuild after a backend error), `true` otherwise.
    #[must_use]
    async fn discover(
        &mut self,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        known_small_files: &mut HashMap<PathBuf, time::Instant>,
        checkpoints: &CheckpointsView,
        notify_discovery: Option<&mut NotifyDiscovery>,
        notify_wakeup: &NotifyWakeup,
    ) -> bool {
        // Defensive resync: cheap to call, and covers the (rare) case where the set of
        // directories implied by `include` patterns needs to change -- e.g. a literal include
        // path's directory didn't exist at startup and now does, or the `PathsProvider`
        // implementation's `watch_roots()` otherwise changes over time. New files created
        // *inside* an already-recursively-watched directory tree don't need this: the OS
        // backend (inotify/FSEvents/ReadDirectoryChangesW) follows new subdirectories on its
        // own once a recursive watch is established on their ancestor.
        let mut keep_notify_discovery = true;
        if let Some(discovery) = notify_discovery {
            keep_notify_discovery =
                discovery.resync_watches(&self.paths_provider.watch_roots(), &self.emitter);
        }

        for (_file_id, watcher) in &mut *fp_map {
            watcher.set_file_findable(false); // assume not findable until found
        }

        // Computed once per pass (not once per file) and only when there's actually a pending
        // notify wakeup to compare against -- the common case, a backstop-timer-only pass with
        // `NotifyWakeup::None`, skips this (and every `.names()` call below) entirely, since
        // `None` never matches regardless of what `path` is compared against.
        let cwd_for_notify_comparison = notify_wakeup
            .is_pending()
            .then(|| std::env::current_dir().ok())
            .flatten();

        for path in self.paths_provider.paths().into_iter() {
            if let Some(file_id) = self
                .fingerprinter
                .fingerprint_or_emit(&path, known_small_files, &self.emitter)
                .await
            {
                if let Some(watcher) = fp_map.get_mut(&file_id) {
                    // file fingerprint matches a watched file
                    let was_found_this_cycle = watcher.file_findable();
                    watcher.set_file_findable(true);
                    if watcher.path == path {
                        trace!(
                            message = "Continue watching file.",
                            path = ?path,
                        );
                        let absolutized_path =
                            crate::absolutize(&path, cwd_for_notify_comparison.as_deref());
                        if notify_wakeup.names(&absolutized_path) {
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
                        watcher.update_path(path).await.ok(); // ok if this fails: might fix next cycle
                    } else {
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
                            watcher.update_path(path).await.ok(); // ok if this fails: might fix next cycle
                        }
                    }
                } else {
                    // untracked file fingerprint
                    self.watch_new_file(path, file_id, fp_map, checkpoints, false)
                        .await;
                    self.emit_open_and_idle_counts(fp_map);
                }
            }
        }
        keep_notify_discovery
    }

    /// Cheaply poll `Idle` watchers (no open file handle) for new data by stat-ing them, reusing
    /// the same discovery cadence (`discover`'s caller) rather than adding a whole separate
    /// polling loop. Promotes any that changed back to `Active` so the read loop picks them up.
    ///
    /// Skips watchers that `discover`'s glob/fingerprint pass just marked unfindable. An `Idle`
    /// watcher holds no handle, so unlike an `Active` one it has no OS-level pin on the specific
    /// inode it was watching: if its old path was renamed away (rotation) and something new was
    /// created at that same path before `rotate_wait` elapses and the stale watcher is reaped, a
    /// stat against `watcher.path` here would be observing the *new* file. Reactivating in that
    /// case would seek the new file to the old, unrelated checkpoint offset -- silently skipping
    /// or re-reading data. Findable watchers are exactly the ones `discover`'s fingerprint match
    /// confirmed still refer to the same file, so only those are safe to promote here.
    async fn poll_idle_watchers(
        &self,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        lines: &mut Vec<Line>,
    ) {
        for (&file_id, watcher) in &mut *fp_map {
            if !watcher.is_idle() || !watcher.file_findable() {
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
                    // Still idle and still unchanged. Idle files are eligible for `remove_after`
                    // cleanup just like active ones, driven off how long they've sat unchanged
                    // rather than "time since last successful read" (which is meaningless for a
                    // watcher that, by construction, isn't reading).
                    if let Some(grace_period) = self.remove_after
                        && watcher
                            .idle_since()
                            .is_some_and(|idle| idle >= grace_period)
                    {
                        match remove_file(&watcher.path).await {
                            Ok(()) => {
                                self.emitter.emit_file_deleted(&watcher.path);
                                salvage_final_partial_line(watcher, file_id, lines);
                                watcher.set_dead();
                            }
                            Err(error) => {
                                self.emitter.emit_file_delete_error(&watcher.path, error);
                            }
                        }
                    }
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
    /// `files_open` reflects only watchers that actually hold an open file handle (`Active`
    /// state); `files_idle` reflects watchers that are tracked (checkpointed, polled) but hold no
    /// handle (`Idle` state). Prior to the idle-watching feature these were always identical to
    /// `fp_map.len()`; splitting them out is what makes the fix for
    /// <https://github.com/vectordotdev/vector/issues/3567> observable.
    fn emit_open_and_idle_counts(&self, fp_map: &IndexMap<FileFingerprint, FileWatcher>) {
        let (mut open, mut idle) = (0usize, 0usize);
        for watcher in fp_map.values() {
            if watcher.is_idle() {
                idle += 1;
            } else {
                open += 1;
            }
        }
        self.emitter.emit_files_open(open);
        self.emitter.emit_files_idle(idle);
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
        assert!(!wakeup.names(&PathBuf::from("/var/log/a.log")));
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
        assert!(wakeup.names(&PathBuf::from("/var/log/a.log")));
        assert!(
            !wakeup.names(&PathBuf::from("/var/log/b.log")),
            "a path the event didn't name must not be reported as needing a nudge"
        );
    }

    #[test]
    fn notify_wakeup_accumulates_paths_across_multiple_add_calls() {
        let mut wakeup = NotifyWakeup::default();
        wakeup.add_paths([PathBuf::from("/var/log/a.log")]);
        wakeup.add_paths([PathBuf::from("/var/log/b.log")]);

        assert!(wakeup.names(&PathBuf::from("/var/log/a.log")));
        assert!(wakeup.names(&PathBuf::from("/var/log/b.log")));
        assert!(!wakeup.names(&PathBuf::from("/var/log/c.log")));
    }

    #[test]
    fn notify_wakeup_mark_all_names_everything() {
        // `Overflow`/`BackendError` don't carry specific paths, so every tracked watcher must be
        // treated as possibly needing a nudge -- this is the pre-existing coarse behavior,
        // preserved for the cases where no finer-grained information is available.
        let mut wakeup = NotifyWakeup::default();
        wakeup.mark_all();

        assert!(wakeup.is_pending());
        assert!(wakeup.names(&PathBuf::from("/var/log/anything.log")));
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

        assert!(matches!(wakeup, NotifyWakeup::All));
        assert!(wakeup.names(&PathBuf::from("/var/log/anything-else.log")));
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
                Duration::from_millis(1),
                discovery_interval,
                rotate_wait,
            ),
            "an idle watcher must not be reaped the instant it's first seen unfindable"
        );
        assert!(
            !should_reap_unfindable_watcher(
                true,
                discovery_interval - Duration::from_millis(1),
                discovery_interval,
                rotate_wait,
            ),
            "an idle watcher must survive at least one full discovery interval unfindable"
        );
        assert!(
            should_reap_unfindable_watcher(
                true,
                discovery_interval + Duration::from_millis(1),
                discovery_interval,
                rotate_wait,
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
                discovery_interval + Duration::from_secs(1),
                discovery_interval,
                rotate_wait,
            ),
            "an active watcher must not be reaped just because a discovery interval elapsed"
        );
        assert!(
            should_reap_unfindable_watcher(
                false,
                rotate_wait + Duration::from_millis(1),
                discovery_interval,
                rotate_wait,
            ),
            "an active watcher must still be reaped once rotate_wait elapses"
        );
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
            !wakeup.names(&glob_discovered_path),
            "sanity check: comparing the raw relative path against the absolute notify path \
             must not match"
        );

        let absolutized = crate::absolutize(&glob_discovered_path, Some(&cwd));
        assert!(
            wakeup.names(&absolutized),
            "after absolutizing the glob-discovered relative path the same way notify resolves \
             its own watch paths, it must match the notify-reported absolute path"
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
