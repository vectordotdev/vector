use std::{
    cmp,
    collections::{BTreeMap, HashMap},
    path::PathBuf,
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
    /// disables this behavior entirely, i.e. active files are never deactivated (the pre-existing
    /// behavior). Applies under both `FileDiscoveryMode::PollingOnly` and
    /// `FileDiscoveryMode::Notify`: notify-based discovery makes finding files fast, but doesn't
    /// by itself stop already-discovered, `ignore_older`-excluded files from holding a handle
    /// open for as long as they exist on disk -- this option is what does that, addressing the
    /// other half of <https://github.com/vectordotdev/vector/issues/3567>.
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
        let using_notify = notify_discovery.is_some();

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
            self.glob_minimum_cooldown,
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
        let mut pending_notify_wakeup = !using_notify; // run one discovery pass unconditionally at loop start when polling
        loop {
            let discovery_interval = if using_notify {
                self.reconcile_interval
            } else {
                self.glob_minimum_cooldown
            };

            // Glob find files to follow, but not too often.
            let now_time = time::Instant::now();
            if next_glob_time <= now_time || pending_notify_wakeup {
                // Schedule the next backstop reconciliation time.
                next_glob_time = now_time.checked_add(discovery_interval).unwrap();
                pending_notify_wakeup = false;

                if stats.started_at.elapsed() > Duration::from_secs(1) {
                    stats.report();
                }

                if stats.started_at.elapsed() > Duration::from_secs(10) {
                    stats = TimingStats::default();
                }

                let start = time::Instant::now();
                self.discover(
                    &mut fp_map,
                    &mut known_small_files,
                    &checkpoints,
                    notify_discovery.as_mut(),
                )
                .await;
                stats.record("discovery", start.elapsed());

                let start = time::Instant::now();
                self.poll_idle_watchers(&mut fp_map).await;
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
                                watcher.set_dead();
                            }
                            Err(error) => {
                                // We will try again after some time.
                                self.emitter.emit_file_delete_error(&watcher.path, error);
                            }
                        }
                    }

                    // The file has reached EOF and produced nothing this
                    // cycle. If it's been quiet (no successful reads) for
                    // `idle_timeout`, close its handle and move it to the
                    // passive `Idle` state: we keep the checkpoint and keep
                    // polling cheaply via `fs::metadata`, but stop holding a
                    // file descriptor open for a file nobody is writing to.
                    // This is the runtime (as opposed to startup) half of the
                    // fix for https://github.com/vectordotdev/vector/issues/3567.
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

            for (_, watcher) in &mut fp_map {
                if !watcher.file_findable() && watcher.last_seen().elapsed() > self.rotate_wait {
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
            let sleep_fut = async move {
                if backoff > 0 {
                    sleep(Duration::from_millis(backoff as u64)).await;
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
                tokio::select! {
                    biased;
                    _ = &mut shutdown_data => {
                        shutdown = true;
                    }
                    msg = discovery.recv() => {
                        match msg {
                            Some(NotifyMessage::PathsChanged(paths)) => {
                                trace!(message = "Received file change notification.", ?paths);
                                pending_notify_wakeup = true;
                            }
                            Some(NotifyMessage::PathsRemoved(paths)) => {
                                trace!(message = "Received file removal notification.", ?paths);
                                pending_notify_wakeup = true;
                            }
                            Some(NotifyMessage::Overflow) => {
                                self.emitter.emit_file_watch_events_overflowed();
                                pending_notify_wakeup = true;
                            }
                            Some(NotifyMessage::BackendError(error)) => {
                                self.emitter.emit_file_watch_backend_error(&std::io::Error::other(error));
                                // A backend error can mean the watcher silently dropped a watch
                                // (e.g. a watched directory was removed and recreated). Forget our
                                // bookkeeping of which directories are watched so the upcoming
                                // reconciliation pass's `resync_watches` call re-`watch`s
                                // everything from scratch, rather than skipping paths it
                                // incorrectly still believes are watched. See
                                // `NotifyDiscovery::forget_watches` for why this is necessary.
                                discovery.forget_watches();
                                pending_notify_wakeup = true;
                            }
                            None => {
                                // The notify watcher task/thread went away entirely (e.g. panicked).
                                // Fall back to relying solely on the backstop reconcile interval
                                // from here on; do not treat this as fatal to the file source.
                                warn!("Notify event channel closed; relying on periodic reconciliation only.");
                                notify_discovery = None;
                            }
                        }
                        // Briefly drain/debounce further events so a burst of writes collapses
                        // into a single reconciliation pass.
                        if let Some(discovery) = notify_discovery.as_mut() {
                            let drain_result =
                                tokio::time::timeout(NOTIFY_EVENT_DEBOUNCE, async {
                                    while discovery.recv().await.is_some() {}
                                })
                                .await;
                            // A timeout just means the debounce window elapsed while events were
                            // still arriving, which is the expected/common case; nothing to do.
                            drop(drain_result);
                        }
                    }
                    _ = &mut sleep_fut => {}
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

    /// Perform a full glob+fingerprint reconciliation pass: re-glob the configured `include`
    /// patterns and detect new files, renames (a known fingerprint appearing at a new path), and
    /// duplicate-fingerprint conflicts (picking the most recently modified file).
    ///
    /// This is the same logic that used to run unconditionally on every `glob_minimum_cooldown`
    /// tick. It's now called either on a fixed interval (`PollingOnly` mode, or as the
    /// `Notify`-mode backstop via `reconcile_interval`), or on-demand when the OS-level notify
    /// watcher reports a change. We deliberately keep this as one unified, full pass rather than
    /// writing a separate "apply this one notify event incrementally" code path: a full pass is
    /// cheap enough to run per-event (it's no longer gated behind a tiny fixed interval baked
    /// into a hot loop), and reusing the already-correct logic avoids a second, potentially
    /// divergent implementation of rename/duplicate-fingerprint handling.
    async fn discover(
        &mut self,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        known_small_files: &mut HashMap<PathBuf, time::Instant>,
        checkpoints: &CheckpointsView,
        notify_discovery: Option<&mut NotifyDiscovery>,
    ) {
        // Defensive resync: cheap to call, and covers the (rare) case where the set of
        // directories implied by `include` patterns needs to change -- e.g. a literal include
        // path's directory didn't exist at startup and now does, or the `PathsProvider`
        // implementation's `watch_roots()` otherwise changes over time. New files created
        // *inside* an already-recursively-watched directory tree don't need this: the OS
        // backend (inotify/FSEvents/ReadDirectoryChangesW) follows new subdirectories on its
        // own once a recursive watch is established on their ancestor.
        if let Some(discovery) = notify_discovery {
            discovery.resync_watches(&self.paths_provider.watch_roots(), &self.emitter);
        }

        for (_file_id, watcher) in &mut *fp_map {
            watcher.set_file_findable(false); // assume not findable until found
        }
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
    async fn poll_idle_watchers(&self, fp_map: &mut IndexMap<FileFingerprint, FileWatcher>) {
        for (_file_id, watcher) in &mut *fp_map {
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
