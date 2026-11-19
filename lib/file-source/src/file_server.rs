use std::{
    cmp,
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use file_source_common::{
    Clock, FileFingerprint, FileSourceInternalEvents, FileSystem, Fingerprinter, MonotonicInstant,
    OsFileSystem, ReadFrom, SystemClock,
    checkpointer::{Checkpointer, CheckpointsView},
};
use futures::{
    Future, Sink, SinkExt,
    future::{Either, select},
};
use futures_util::future::join_all;
use indexmap::IndexMap;
use tokio_util::task::JoinMap;
use tracing::{debug, error, info, trace};

use crate::{
    file_watcher::{FileWatcher, RawLineResult},
    paths_provider::PathsProvider,
    scheduler::{FsEvent, FsEventSource, LegacyPollingScheduler, NoopFsEventSource, ReadScheduler},
};

/// `FileServer` is a Source which cooperatively schedules reads over files,
/// converting the lines of said files into `LogLine` structures. As
/// `FileServer` is intended to be useful across multiple operating systems with
/// POSIX filesystem semantics `FileServer` must poll for changes. That is, no
/// event notification is used by `FileServer`.
///
/// `FileServer` is configured on a path to watch. The files do _not_ need to
/// exist at startup. `FileServer` will discover new files which match
/// its path in at most 60 seconds.
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
}

/// Steppable runtime state of a [`FileServer`], parameterized over [`FileSystem`],
/// [`Clock`], [`ReadScheduler`], and [`FsEventSource`] to enable deterministic
/// simulation testing and pluggable scheduling without modifying production behavior.
pub struct FileServerState<
    PP: PathsProvider,
    E: FileSourceInternalEvents,
    FS: FileSystem = OsFileSystem,
    C: Clock = SystemClock,
    S: ReadScheduler = LegacyPollingScheduler,
    Ev: FsEventSource = NoopFsEventSource,
> {
    server: FileServer<PP, E>,
    fs: FS,
    clock: C,
    scheduler: S,
    event_source: Ev,
    fp_map: IndexMap<FileFingerprint, FileWatcher<FS>>,
    known_small_files: HashMap<PathBuf, MonotonicInstant>,
    checkpoints: Arc<CheckpointsView<C>>,
    backoff_cap: usize,
    next_glob_time: MonotonicInstant,
    stats: TimingStats,
    pending_events: Vec<FsEvent>,
    read_candidates: Vec<FileFingerprint>,
}

impl<PP, E, FS, C> FileServerState<PP, E, FS, C, LegacyPollingScheduler, NoopFsEventSource>
where
    PP: PathsProvider,
    E: FileSourceInternalEvents,
    FS: FileSystem,
    C: Clock,
{
    /// Initialize `FileServerState` with the default [`LegacyPollingScheduler`]
    /// and [`NoopFsEventSource`].
    pub async fn init(
        server: FileServer<PP, E>,
        checkpointer: &mut Checkpointer<FS, C>,
        fs: FS,
        clock: C,
    ) -> Self {
        Self::init_with_scheduler(
            server,
            checkpointer,
            fs,
            clock,
            LegacyPollingScheduler::default(),
            NoopFsEventSource,
        )
        .await
    }
}

impl<PP, E, FS, C, S, Ev> FileServerState<PP, E, FS, C, S, Ev>
where
    PP: PathsProvider,
    E: FileSourceInternalEvents,
    FS: FileSystem,
    C: Clock,
    S: ReadScheduler,
    Ev: FsEventSource,
{
    /// Initialize `FileServerState` with an explicit [`ReadScheduler`] and [`FsEventSource`].
    pub async fn init_with_scheduler(
        mut server: FileServer<PP, E>,
        checkpointer: &mut Checkpointer<FS, C>,
        fs: FS,
        clock: C,
        scheduler: S,
        event_source: Ev,
    ) -> Self {
        let fp_map: IndexMap<FileFingerprint, FileWatcher<FS>> = IndexMap::default();
        let mut known_small_files = HashMap::new();

        checkpointer.read_checkpoints(server.ignore_before).await;

        let now = clock.now();
        let mut existing_files = Vec::new();
        for path in server.paths_provider.paths() {
            if let Some(file_id) = server
                .fingerprinter
                .fingerprint_or_emit_with_fs(
                    &path,
                    &mut known_small_files,
                    &server.emitter,
                    &fs,
                    now,
                )
                .await
            {
                existing_files.push((path, file_id));
            }
        }

        let metadata = join_all(
            existing_files
                .iter()
                .map(|(path, _file_id)| fs.metadata(path)),
        )
        .await;

        let now_utc = clock.now_utc();
        let created = metadata
            .into_iter()
            .map(|m| m.ok().and_then(|m| m.created).unwrap_or(now_utc));

        let mut existing_files: Vec<(DateTime<Utc>, PathBuf, FileFingerprint)> = existing_files
            .into_iter()
            .zip(created)
            .map(|((path, file_id), key)| (key, path, file_id))
            .collect();

        existing_files.sort_by_key(|(key, _, _)| *key);

        let checkpoints = checkpointer.view();

        let mut state = Self {
            server,
            fs,
            clock,
            scheduler,
            event_source,
            fp_map,
            known_small_files,
            checkpoints,
            backoff_cap: 1,
            next_glob_time: now,
            stats: TimingStats::new(now),
            pending_events: Vec::new(),
            read_candidates: Vec::new(),
        };

        for (_key, path, file_id) in existing_files {
            state.watch_new_file(path, file_id, true).await;
        }
        state.server.emitter.emit_files_open(state.fp_map.len());

        state
    }

    /// Returns the number of currently watched open files.
    #[must_use]
    pub fn open_file_count(&self) -> usize {
        self.fp_map.len()
    }

    /// Returns a handle to the shared checkpoints view.
    #[must_use]
    pub fn checkpoints(&self) -> &Arc<CheckpointsView<C>> {
        &self.checkpoints
    }

    /// Execute one iteration of the `FileServer` loop: optional discovery,
    /// small-file cleanup, file polling, dead-watcher reaping, and backoff
    /// calculation. Returns the backoff duration to sleep before the next step.
    ///
    /// # Panics
    /// Panics if the polling deadline overflows or a file path is not UTF-8.
    pub async fn step(&mut self, lines: &mut Vec<Line>) -> Duration {
        let now_time = self.clock.now();

        // Drain any pending OS/simulated filesystem notifications and feed them to the scheduler.
        self.pending_events.clear();
        self.event_source.drain_events(&mut self.pending_events);
        for event in &self.pending_events {
            self.scheduler.on_fs_event(event, now_time);
        }

        if self.next_glob_time <= now_time {
            self.next_glob_time = now_time
                .checked_add(self.server.glob_minimum_cooldown)
                .unwrap();

            if now_time.saturating_duration_since(self.stats.started_at) > Duration::from_secs(1) {
                self.stats.report(now_time);
            }

            if now_time.saturating_duration_since(self.stats.started_at) > Duration::from_secs(10) {
                self.stats = TimingStats::new(now_time);
            }

            self.discover_files().await;
        }

        self.cleanup_small_files().await;

        let lines_start = lines.len();
        let global_bytes_read = self.poll_files(lines).await;

        self.cleanup_idle_files(&lines[lines_start..]).await;

        self.reap_dead_files().await;

        // When no lines have been read we kick the backoff_cap up by twice,
        // limited by the hard-coded cap. Else, we set the backoff_cap to its
        // minimum on the assumption that next time through there will be
        // more lines to read promptly.
        self.backoff_cap = if global_bytes_read == 0 {
            cmp::min(2_048, self.backoff_cap.saturating_mul(2))
        } else {
            1
        };
        let backoff = self.backoff_cap.saturating_sub(global_bytes_read);
        Duration::from_millis(backoff as u64)
    }

    /// Search (glob) for files to detect new files, renames, and fingerprint collisions.
    pub async fn discover_files(&mut self) {
        let start = self.clock.now();
        for (_file_id, watcher) in &mut self.fp_map {
            watcher.set_file_findable_at(false, start);
        }

        let paths: Vec<PathBuf> = self.server.paths_provider.paths().into_iter().collect();
        for path in paths {
            let now = self.clock.now();
            if let Some(file_id) = self
                .server
                .fingerprinter
                .fingerprint_or_emit_with_fs(
                    &path,
                    &mut self.known_small_files,
                    &self.server.emitter,
                    &self.fs,
                    now,
                )
                .await
            {
                if let Some(watcher) = self.fp_map.get_mut(&file_id) {
                    // file fingerprint matches a watched file
                    let was_found_this_cycle = watcher.file_findable();
                    watcher.set_file_findable_at(true, now);
                    if watcher.path == path {
                        trace!(
                            message = "Continue watching file.",
                            path = ?path,
                        );
                    } else if !was_found_this_cycle {
                        // matches a file with a different path
                        let old_path = watcher.path.clone();
                        info!(
                            message = "Watched file has been renamed.",
                            path = ?path,
                            old_path = ?old_path
                        );
                        if let Ok(unwatch_opt) = watcher.update_path(path.clone()).await {
                            self.event_source.unwatch_path(&old_path);
                            self.event_source.watch_path(&path);
                            self.scheduler
                                .on_file_renamed(file_id, &old_path, &path, now);
                            if let Some(unwatch_info) = unwatch_opt {
                                // Inode changed - emit metrics for the old file
                                self.server.emitter.emit_file_unwatched(
                                    &unwatch_info.path,
                                    unwatch_info.reached_eof,
                                    unwatch_info.bytes_unread,
                                );
                            }
                        }
                    } else {
                        info!(
                            message = "More than one file has the same fingerprint.",
                            path = ?path,
                            old_path = ?watcher.path
                        );
                        let (old_path, new_path) = (watcher.path.clone(), path.clone());
                        if let (Ok(old_meta), Ok(new_meta)) = (
                            self.fs.metadata(&old_path).await,
                            self.fs.metadata(&new_path).await,
                        ) && let (Some(old_modified_time), Some(new_modified_time)) =
                            (old_meta.modified, new_meta.modified)
                            && old_modified_time < new_modified_time
                        {
                            info!(
                                message = "Switching to watch most recently modified file.",
                                new_modified_time = ?new_modified_time,
                                old_modified_time = ?old_modified_time,
                            );
                            if let Ok(unwatch_opt) = watcher.update_path(path).await {
                                self.event_source.unwatch_path(&old_path);
                                self.event_source.watch_path(&new_path);
                                self.scheduler
                                    .on_file_renamed(file_id, &old_path, &new_path, now);
                                if let Some(unwatch_info) = unwatch_opt {
                                    // Inode changed - emit metrics for the old file
                                    self.server.emitter.emit_file_unwatched(
                                        &unwatch_info.path,
                                        unwatch_info.reached_eof,
                                        unwatch_info.bytes_unread,
                                    );
                                }
                            }
                        }
                    }
                } else {
                    // untracked file fingerprint
                    self.watch_new_file(path, file_id, false).await;
                    self.server.emitter.emit_files_open(self.fp_map.len());
                }
            }
        }
        self.stats.record(
            "discovery",
            self.clock.now().saturating_duration_since(start),
        );
    }

    /// Cleanup `known_small_files` that have exceeded `remove_after`.
    pub async fn cleanup_small_files(&mut self) {
        if let Some(grace_period) = self.server.remove_after {
            let now = self.clock.now();
            let mut set = JoinMap::new();

            self.known_small_files
                .iter()
                .filter(|&(_path, last_time_open)| {
                    now.saturating_duration_since(*last_time_open) >= grace_period
                })
                .map(|(path, _last_time_open)| path.clone())
                .for_each(|path| {
                    let fs = self.fs.clone();
                    let p = path.clone();
                    set.spawn(path, async move { fs.remove_file(&p).await });
                });

            while let Some((path, result)) = set.join_next().await {
                match result.map_err(std::io::Error::other).flatten() {
                    Ok(()) => {
                        let removed = self.known_small_files.remove(&path);

                        if removed.is_some() {
                            self.server.emitter.emit_file_deleted(&path);
                        }
                    }
                    Err(err) => {
                        self.server.emitter.emit_file_delete_error(&path, err);
                    }
                }
            }
        }
    }

    /// Collect lines by polling watched files selected by the [`ReadScheduler`].
    /// Returns total bytes read across all files.
    ///
    /// # Panics
    /// Panics if a watched file path is not valid UTF-8.
    pub async fn poll_files(&mut self, lines: &mut Vec<Line>) -> usize {
        let mut global_bytes_read: usize = 0;
        let mut maxed_out_reading_single_file = false;

        self.read_candidates.clear();
        let select_now = self.clock.now();
        self.scheduler
            .select_readable(select_now, &mut self.read_candidates);
        let candidates = std::mem::take(&mut self.read_candidates);

        for (idx, &file_id) in candidates.iter().enumerate() {
            let Some(watcher) = self.fp_map.get_mut(&file_id) else {
                continue;
            };

            let start = self.clock.now();
            let mut bytes_read: usize = 0;
            let mut hit_max_read_bytes = false;
            while let Ok(RawLineResult {
                raw_line: Some(line),
                discarded_for_size_and_truncated,
            }) = watcher.read_line_at(self.clock.now()).await
            {
                for buf in &discarded_for_size_and_truncated {
                    self.server.emitter.emit_file_line_too_long(
                        &buf.clone(),
                        self.server.max_line_bytes,
                        buf.len(),
                    );
                }

                let sz = line.bytes.len();
                trace!(
                    message = "Read bytes.",
                    path = ?watcher.path,
                    bytes = ?sz
                );
                self.stats.record_bytes(sz);

                bytes_read += sz;

                lines.push(Line {
                    text: line.bytes,
                    filename: watcher.path.to_str().expect("not a valid path").to_owned(),
                    file_id,
                    start_offset: line.offset,
                    end_offset: watcher.get_file_position(),
                });

                if bytes_read > self.server.max_read_bytes {
                    hit_max_read_bytes = true;
                    maxed_out_reading_single_file = true;
                    break;
                }
            }
            let after_read = self.clock.now();
            self.stats
                .record("reading", after_read.saturating_duration_since(start));

            if bytes_read > 0 {
                global_bytes_read = global_bytes_read.saturating_add(bytes_read);
                self.scheduler.on_file_read(file_id, bytes_read, after_read);
                if !hit_max_read_bytes {
                    self.scheduler.on_file_eof(file_id, after_read);
                }
            } else {
                self.scheduler.on_file_eof(file_id, after_read);
            }

            // Do not move on to newer files if we are behind on an older file
            if self.server.oldest_first && maxed_out_reading_single_file {
                for &skipped_id in &candidates[idx + 1..] {
                    self.scheduler.on_file_skipped(skipped_id);
                }
                break;
            }
        }
        self.read_candidates = candidates;
        global_bytes_read
    }

    /// Remove idle watched files that have exceeded `remove_after`, decoupled from
    /// whether the [`ReadScheduler`] polled them on this tick.
    async fn cleanup_idle_files(&mut self, lines_read_this_tick: &[Line]) {
        let Some(grace_period) = self.server.remove_after else {
            return;
        };
        let now = self.clock.now();
        for (&file_id, watcher) in &mut self.fp_map {
            if watcher.dead() {
                continue;
            }
            if lines_read_this_tick.iter().any(|l| l.file_id == file_id) {
                continue;
            }
            if now.saturating_duration_since(watcher.last_read_success()) >= grace_period {
                match self.fs.remove_file(&watcher.path).await {
                    Ok(()) => {
                        self.server.emitter.emit_file_deleted(&watcher.path);
                        watcher.set_dead();
                    }
                    Err(error) => {
                        // We will try again after some time.
                        self.server
                            .emitter
                            .emit_file_delete_error(&watcher.path, error);
                    }
                }
            }
        }
    }

    /// Expire unfindable watchers past `rotate_wait` and remove all dead watchers.
    pub async fn reap_dead_files(&mut self) {
        let now = self.clock.now();
        for (_, watcher) in &mut self.fp_map {
            if !watcher.file_findable()
                && now.saturating_duration_since(watcher.last_seen()) > self.server.rotate_wait
            {
                watcher.set_dead();
            }
        }

        // A FileWatcher is dead when the underlying file has disappeared.
        // If the FileWatcher is dead we don't retain it; it will be deallocated.
        // First collect dead file IDs, then process them (get_unwatch_info is async).
        let dead_file_ids: Vec<_> = self
            .fp_map
            .iter()
            .filter(|(_, watcher)| watcher.dead())
            .map(|(file_id, _)| *file_id)
            .collect();

        for file_id in dead_file_ids {
            if let Some(watcher) = self.fp_map.shift_remove(&file_id) {
                self.scheduler.on_file_removed(file_id);
                self.event_source.unwatch_path(&watcher.path);
                let unwatch_info = watcher.get_unwatch_info().await;
                self.server.emitter.emit_file_unwatched(
                    &unwatch_info.path,
                    unwatch_info.reached_eof,
                    unwatch_info.bytes_unread,
                );
                self.checkpoints.set_dead(file_id);
            }
        }
        self.server.emitter.emit_files_open(self.fp_map.len());
    }

    async fn watch_new_file(&mut self, path: PathBuf, file_id: FileFingerprint, startup: bool) {
        // Determine the initial _requested_ starting point in the file. This can be overridden
        // once the file is actually opened and we determine it is compressed, older than we're
        // configured to read, etc.
        let fallback = if startup {
            self.server.read_from
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
        let read_from = if self.server.ignore_checkpoints {
            fallback
        } else {
            self.checkpoints
                .get(file_id)
                .map_or(fallback, ReadFrom::Checkpoint)
        };

        let now = self.clock.now();
        let now_utc = self.clock.now_utc();
        match FileWatcher::new_with_fs(
            path.clone(),
            read_from,
            self.server.ignore_before,
            self.server.max_line_bytes,
            self.server.line_delimiter.clone(),
            self.fs.clone(),
            now,
            now_utc,
        )
        .await
        {
            Ok(mut watcher) => {
                if let ReadFrom::Checkpoint(file_position) = read_from {
                    self.server.emitter.emit_file_resumed(&path, file_position);
                } else {
                    self.server.emitter.emit_file_added(&path);
                }
                watcher.set_file_findable_at(true, now);
                let initial_ts = watcher.last_read_success();
                self.fp_map.insert(file_id, watcher);
                self.event_source.watch_path(&path);
                self.scheduler
                    .on_file_added(file_id, &path, initial_ts, now);
            }
            Err(error) => self.server.emitter.emit_file_watch_error(&path, error),
        }
    }
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
/// Specific operating systems support evented interfaces that correct this
/// problem but your intrepid authors know of no generic solution.
impl<PP, E> FileServer<PP, E>
where
    PP: PathsProvider,
    E: FileSourceInternalEvents,
{
    /// Read watched files until shutdown, forwarding their lines to the output sink.
    ///
    /// `shutdown_data` stops output; `shutdown_checkpointer` stops the background
    /// checkpoint writer after all acknowledgements have completed.
    ///
    /// # Errors
    /// Returns the output sink's error if sending a batch of lines fails.
    ///
    /// # Panics
    /// Panics if the polling deadline overflows, a file path is not UTF-8,
    /// closing the output sink fails, or the checkpoint writer task fails.
    /// Creating a watcher can also panic as described by [`FileWatcher::new`].
    /// Requires a Tokio runtime with timers enabled.
    pub async fn run<C, S1, S2>(
        self,
        chans: C,
        shutdown_data: S1,
        shutdown_checkpointer: S2,
        checkpointer: Checkpointer,
    ) -> Result<Shutdown, <C as Sink<Vec<Line>>>::Error>
    where
        C: Sink<Vec<Line>> + Unpin,
        <C as Sink<Vec<Line>>>::Error: std::error::Error,
        S1: Future + Unpin + Send + 'static,
        S2: Future + Unpin + Send + 'static,
    {
        self.run_with(
            chans,
            shutdown_data,
            shutdown_checkpointer,
            checkpointer,
            OsFileSystem,
            SystemClock,
        )
        .await
    }

    /// Run the `FileServer` with an explicit [`FileSystem`] and [`Clock`].
    ///
    /// # Errors
    /// Returns the output sink's error if sending a batch of lines fails.
    ///
    /// # Panics
    /// Panics if the polling deadline overflows, a file path is not UTF-8,
    /// closing the output sink fails, or the checkpoint writer task fails.
    pub async fn run_with<Ch, S1, S2, FS, Clk>(
        self,
        chans: Ch,
        shutdown_data: S1,
        shutdown_checkpointer: S2,
        checkpointer: Checkpointer<FS, Clk>,
        fs: FS,
        clock: Clk,
    ) -> Result<Shutdown, <Ch as Sink<Vec<Line>>>::Error>
    where
        Ch: Sink<Vec<Line>> + Unpin,
        <Ch as Sink<Vec<Line>>>::Error: std::error::Error,
        S1: Future + Unpin + Send + 'static,
        S2: Future + Unpin + Send + 'static,
        FS: FileSystem,
        Clk: Clock,
    {
        self.run_with_scheduler(
            chans,
            shutdown_data,
            shutdown_checkpointer,
            checkpointer,
            fs,
            clock,
            LegacyPollingScheduler::default(),
            NoopFsEventSource,
        )
        .await
    }

    /// Run the `FileServer` with explicit [`FileSystem`], [`Clock`], [`ReadScheduler`],
    /// and [`FsEventSource`]. If the event source provides a wakeup notifier, idle
    /// backoff sleeps are interrupted immediately when OS filesystem events arrive.
    ///
    /// # Errors
    /// Returns the output sink's error if sending a batch of lines fails.
    ///
    /// # Panics
    /// Panics if the polling deadline overflows, a file path is not UTF-8,
    /// closing the output sink fails, or the checkpoint writer task fails.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_with_scheduler<Ch, S1, S2, FS, Clk, S, Ev>(
        self,
        mut chans: Ch,
        mut shutdown_data: S1,
        shutdown_checkpointer: S2,
        mut checkpointer: Checkpointer<FS, Clk>,
        fs: FS,
        clock: Clk,
        scheduler: S,
        event_source: Ev,
    ) -> Result<Shutdown, <Ch as Sink<Vec<Line>>>::Error>
    where
        Ch: Sink<Vec<Line>> + Unpin,
        <Ch as Sink<Vec<Line>>>::Error: std::error::Error,
        S1: Future + Unpin + Send + 'static,
        S2: Future + Unpin + Send + 'static,
        FS: FileSystem,
        Clk: Clock,
        S: ReadScheduler,
        Ev: FsEventSource,
    {
        let mut lines = Vec::new();
        let wakeup = event_source.wakeup_notifier();
        let mut state = FileServerState::init_with_scheduler(
            self,
            &mut checkpointer,
            fs,
            clock.clone(),
            scheduler,
            event_source,
        )
        .await;

        // Spawn the checkpoint writer task
        let checkpoint_task_handle = vector_common::spawn_in_current_span(checkpoint_writer(
            checkpointer,
            state.server.glob_minimum_cooldown,
            shutdown_checkpointer,
            state.server.emitter.clone(),
            clock.clone(),
        ));

        loop {
            let backoff = state.step(&mut lines).await;

            let start = clock.now();
            let to_send = std::mem::take(&mut lines);

            let result = chans.send(to_send).await;
            match result {
                Ok(()) => {}
                Err(error) => {
                    error!(message = "Output channel closed.", %error);
                    return Err(error);
                }
            }
            state
                .stats
                .record("sending", clock.now().saturating_duration_since(start));

            let start = clock.now();
            let sleep_clock = clock.clone();
            let sleep_wakeup = wakeup.clone();
            let sleep = async move {
                if !backoff.is_zero() {
                    if let Some(notify) = sleep_wakeup {
                        tokio::select! {
                            () = sleep_clock.sleep(backoff) => {},
                            () = notify.notified() => {},
                        }
                    } else {
                        sleep_clock.sleep(backoff).await;
                    }
                }
            };
            futures::pin_mut!(sleep);
            match select(shutdown_data, sleep).await {
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
                Either::Right(((), future)) => shutdown_data = future,
            }
            state
                .stats
                .record("sleeping", clock.now().saturating_duration_since(start));
        }
    }
}

async fn checkpoint_writer<FS: FileSystem, Clk: Clock>(
    checkpointer: Checkpointer<FS, Clk>,
    sleep_duration: Duration,
    mut shutdown: impl Future + Unpin,
    emitter: impl FileSourceInternalEvents,
    clock: Clk,
) -> Arc<Checkpointer<FS, Clk>> {
    let checkpointer = Arc::new(checkpointer);
    loop {
        let sleep = clock.sleep(sleep_duration);
        tokio::select! {
            _ = &mut shutdown => break,
            () = sleep => {},
        }

        let emitter = emitter.clone();
        let checkpointer = Arc::clone(&checkpointer);
        let start = clock.now();
        match checkpointer.write_checkpoints().await {
            Ok(count) => {
                emitter.emit_file_checkpointed(count, clock.now().saturating_duration_since(start));
            }
            Err(error) => emitter.emit_file_checkpoint_write_error(error),
        }
    }
    checkpointer
}

#[must_use]
// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::cast_possible_wrap,
    reason = "Preserve the existing duration conversion until overflow handling is audited."
)]
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
    started_at: MonotonicInstant,
    segments: BTreeMap<&'static str, Duration>,
    events: usize,
    bytes: usize,
}

impl TimingStats {
    fn new(started_at: MonotonicInstant) -> Self {
        Self {
            started_at,
            segments: BTreeMap::default(),
            events: 0,
            bytes: 0,
        }
    }

    fn record(&mut self, key: &'static str, duration: Duration) {
        let segment = self.segments.entry(key).or_default();
        *segment += duration;
    }

    fn record_bytes(&mut self, bytes: usize) {
        self.events += 1;
        self.bytes += bytes;
    }

    fn report(&self, now: MonotonicInstant) {
        if !tracing::level_enabled!(tracing::Level::DEBUG) {
            return;
        }
        let total = now.saturating_duration_since(self.started_at);
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

// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::cast_precision_loss,
    reason = "Preserve the existing floating-point precision of diagnostic throughput formatting."
)]
fn scale(bytes: u64) -> String {
    let units = ["", "k", "m", "g"];
    let mut bytes = bytes as f32;
    let mut i = 0;
    while bytes > 1000.0 && i <= 3 {
        bytes /= 1000.0;
        i += 1;
    }
    format!("{bytes:.3}{}/sec", units[i])
}

#[derive(Debug)]
pub struct Line {
    pub text: Bytes,
    pub filename: String,
    pub file_id: FileFingerprint,
    pub start_offset: u64,
    pub end_offset: u64,
}
