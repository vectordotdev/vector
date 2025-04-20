use std::{time::Instant,
    cmp,
    collections::{BTreeMap, HashSet},
    fs::{self, remove_file},
    path::PathBuf,
    sync::Arc,
    time::{self, Duration},
};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{
    future::{select, Either},
    Future, Sink, SinkExt,
};
use indexmap::IndexMap;
use tokio::time::sleep;
use tracing::{debug, error, info, trace};

use crate::{
    checkpointer::{Checkpointer, CheckpointsView},
    file_watcher::FileWatcher,
    fingerprinter::{FileFingerprint, Fingerprinter},
    paths_provider::PathsProvider,
    FileSourceInternalEvents, ReadFrom,
};

/// `FileServer` is a Source which schedules reads over files,
/// converting the lines of said files into `LogLine` structures.
///
/// `FileServer` uses a hybrid approach for file monitoring:
/// 1. Active polling: For files that are actively being read
/// 2. Passive watching: For idle files, using filesystem notifications via notify-rs/notify
///
/// This approach allows `FileServer` to efficiently monitor many files without
/// holding open file handles for all of them, while still maintaining checkpoints
/// in case files receive future writes.
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
    pub handle: tokio::runtime::Handle,
    pub rotate_wait: Duration,
    /// Whether we're using notify-based file discovery
    pub using_notify_discovery: bool,
    /// Duration after which to checkpoint files
    pub checkpoint_interval: Duration,
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
    // The first `shutdown_data` signal here is to stop this file
    // server from outputting new data; the second
    // `shutdown_checkpointer` is for finishing the background
    // checkpoint writer task, which has to wait for all
    // acknowledgements to be completed.
    pub fn run<C, S1, S2>(
        self,
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
        // We're using notify-based watching for all files
        // This means we only need to glob once on startup to discover files
        // that were added while Vector was stopped
        debug!(message = "Using notify-based watching for all files");

        let mut fingerprint_buffer = Vec::new();

        let mut fp_map: IndexMap<FileFingerprint, FileWatcher> = Default::default();

        let mut backoff_cap: usize = 1;
        let mut lines = Vec::new();

        checkpointer.read_checkpoints(self.ignore_before);

        let mut known_small_files = HashSet::new();

        let mut existing_files = Vec::new();
        // Use block_on to call the async paths method
        let paths = self.handle.block_on(self.paths_provider.paths());
        for path in paths.into_iter() {
            if let Some(file_id) = self.fingerprinter.get_fingerprint_or_log_error(
                &path,
                &mut fingerprint_buffer,
                &mut known_small_files,
                &self.emitter,
            ) {
                existing_files.push((path, file_id));
            }
        }

        existing_files.sort_by_key(|(path, _file_id)| {
            fs::metadata(path)
                .and_then(|m| m.created())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now())
        });

        let checkpoints = checkpointer.view();

        for (path, file_id) in existing_files {
            checkpointer.maybe_upgrade(
                &path,
                file_id,
                &self.fingerprinter,
                &mut fingerprint_buffer,
            );

            self.watch_new_file(path, file_id, &mut fp_map, &checkpoints, true, &mut lines);
        }
        self.emitter.emit_files_open(fp_map.len());

        let mut stats = TimingStats::default();

        // Spawn the checkpoint writer task with appropriate interval
        let checkpoint_interval = if self.using_notify_discovery {
            // When using notify-based discovery, we can use a longer checkpoint interval
            // since we're not relying on frequent polling
            self.checkpoint_interval
        } else {
            // Standard behavior for polling-based discovery
            self.glob_minimum_cooldown
        };

        let checkpoint_task_handle = self.handle.spawn(checkpoint_writer(
            checkpointer,
            checkpoint_interval,
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
        let mut next_glob_time = time::Instant::now();
        loop {
            // Determine if we need to perform file discovery
            let now_time = time::Instant::now();
            let should_discover = if self.using_notify_discovery {
                // When using notify-based discovery, we want to check for new files frequently
                // to minimize the delay between when a file is discovered by the notify watcher
                // and when it's actually processed, but not on every iteration to avoid excessive CPU usage
                next_glob_time <= now_time || now_time.duration_since(next_glob_time) > Duration::from_millis(100)
            } else {
                // Standard behavior for polling-based discovery
                next_glob_time <= now_time
            };

            // Report stats periodically, but only if enough time has passed since the last report
            // This prevents excessive logging when the main loop is running frequently
            static mut LAST_STATS_REPORT: Option<Instant> = None;
            let now = Instant::now();
            let should_report_stats = unsafe {
                if let Some(last_report) = LAST_STATS_REPORT {
                    if now.duration_since(last_report) >= Duration::from_secs(10) {
                        LAST_STATS_REPORT = Some(now);
                        true
                    } else {
                        false
                    }
                } else {
                    LAST_STATS_REPORT = Some(now);
                    true
                }
            };

            if should_report_stats && stats.started_at.elapsed() > Duration::from_secs(10) {
                stats.report();
            }

            if stats.started_at.elapsed() > Duration::from_secs(10) {
                stats = TimingStats::default();
            }

            if should_discover {
                // Schedule the next glob time.
                next_glob_time = now_time.checked_add(self.glob_minimum_cooldown).unwrap();

                // Search (glob) for files to detect major file changes.
                let start = time::Instant::now();
                for (_file_id, watcher) in &mut fp_map {
                    watcher.set_file_findable(false); // assume not findable until found
                }

                // Use async paths provider
                let paths = self.handle.block_on(self.paths_provider.paths());
                for path in paths.into_iter() {
                    if let Some(file_id) = self.fingerprinter.get_fingerprint_or_log_error(
                        &path,
                        &mut fingerprint_buffer,
                        &mut known_small_files,
                        &self.emitter,
                    ) {
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
                                watcher.update_path(path).ok(); // ok if this fails: might fix next cycle
                            } else {
                                info!(
                                    message = "More than one file has the same fingerprint.",
                                    path = ?path,
                                    old_path = ?watcher.path
                                );
                                let (old_path, new_path) = (&watcher.path, &path);
                                if let (Ok(old_modified_time), Ok(new_modified_time)) = (
                                    fs::metadata(old_path).and_then(|m| m.modified()),
                                    fs::metadata(new_path).and_then(|m| m.modified()),
                                ) {
                                    if old_modified_time < new_modified_time {
                                        info!(
                                            message = "Switching to watch most recently modified file.",
                                            new_modified_time = ?new_modified_time,
                                            old_modified_time = ?old_modified_time,
                                        );
                                        watcher.update_path(path).ok(); // ok if this fails: might fix next cycle
                                    }
                                }
                            }
                        } else {
                            // untracked file fingerprint
                            // Immediately watch and read the new file
                            let path_clone = path.clone();
                            debug!(message = "Discovered new file during runtime", ?path_clone);
                            self.watch_new_file(path, file_id, &mut fp_map, &checkpoints, false, &mut lines);

                            // Immediately read the file to avoid delay in detecting content
                            if let Some(watcher) = fp_map.get_mut(&file_id) {
                                debug!(message = "Immediately reading newly discovered file", ?path_clone);
                                let mut bytes_read: usize = 0;
                                while let Ok(Some(line)) = watcher.read_line() {
                                    let sz = line.bytes.len();
                                    trace!(message = "Read bytes from new file", ?path_clone, bytes = ?sz);
                                    bytes_read += sz;

                                    lines.push(Line {
                                        text: line.bytes,
                                        filename: watcher.path.to_str().expect("not a valid path").to_owned(),
                                        file_id,
                                        start_offset: line.offset,
                                        end_offset: watcher.get_file_position(),
                                    });
                                }

                                if bytes_read > 0 {
                                    debug!(message = "Read initial content from newly discovered file", ?path_clone, bytes = bytes_read);
                                }
                            }
                            self.emitter.emit_files_open(fp_map.len());
                        }
                    }
                }
                stats.record("discovery", start.elapsed());
            }

            // Collect lines by polling files.
            let mut global_bytes_read: usize = 0;
            let mut maxed_out_reading_single_file = false;
            for (&file_id, watcher) in &mut fp_map {

                let start = time::Instant::now();
                let mut bytes_read: usize = 0;
                while let Ok(Some(line)) = watcher.read_line() {
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
                    if let Some(grace_period) = self.remove_after {
                        if watcher.last_read_success().elapsed() >= grace_period {
                            // Try to remove
                            match remove_file(&watcher.path) {
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
                    }
                }

                // Do not move on to newer files if we are behind on an older file
                if self.oldest_first && maxed_out_reading_single_file {
                    break;
                }
            }

            // Handle file watcher state transitions
            for (_, watcher) in &mut fp_map {
                // Mark files as dead if they're not findable and have been missing for too long
                if !watcher.file_findable() && watcher.last_seen().elapsed() > self.rotate_wait {
                    watcher.set_dead();
                    continue;
                }

                // Only update the watcher if we've read data from the file
                // This avoids unnecessary updates that can cause excessive logging
                if watcher.reached_eof() && watcher.last_read_success().elapsed() < Duration::from_secs(1) {
                    // Update the notify watcher with the current position
                    if let Err(e) = watcher.update_watcher() {
                        debug!(
                            message = "Failed to update watcher",
                            ?watcher.path,
                            error = ?e
                        );
                    }
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
            self.emitter.emit_files_open(fp_map.len());

            let start = time::Instant::now();
            let to_send = std::mem::take(&mut lines);
            let result = self.handle.block_on(chans.send(to_send));
            match result {
                Ok(()) => {}
                Err(error) => {
                    error!(message = "Output channel closed.", %error);
                    return Err(error);
                }
            }
            stats.record("sending", start.elapsed());

            let start = time::Instant::now();
            // Determine the appropriate backoff based on file activity and discovery mode
            let backoff = if self.using_notify_discovery {
                // When using notify-based watching for all files, use minimal backoff
                // This ensures we detect changes immediately
                // 1ms is the minimum possible value to ensure immediate responsiveness
                1
            } else {
                // Standard behavior for polling-based discovery
                backoff_cap = if global_bytes_read == 0 {
                    cmp::min(2_048, backoff_cap.saturating_mul(2))
                } else {
                    1
                };
                backoff_cap.saturating_sub(global_bytes_read)
            };

            // This works only if run inside tokio context since we are using
            // tokio's Timer. Outside of such context, this will panic on the first
            // call. Also since we are using block_on here and in the above code,
            // this should be run in its own thread. `spawn_blocking` fulfills
            // all of these requirements.
            let sleep = async move {
                if backoff > 0 {
                    // Always use a very short sleep duration to ensure responsiveness
                    // This is especially important for notify-based watching
                    sleep(Duration::from_millis(1)).await;
                }
            };
            futures::pin_mut!(sleep);
            match self.handle.block_on(select(shutdown_data, sleep)) {
                Either::Left((_, _)) => {
                    // Shut down all file watchers to prevent further events
                    debug!(message = "Shutting down all file watchers", count = fp_map.len());
                    for (_, watcher) in fp_map.iter_mut() {
                        watcher.shutdown();
                    }

                    self.handle
                        .block_on(chans.close())
                        .expect("error closing file_server data channel.");
                    let checkpointer = self
                        .handle
                        .block_on(checkpoint_task_handle)
                        .expect("checkpoint task has panicked");
                    if let Err(error) = checkpointer.write_checkpoints() {
                        error!(?error, "Error writing checkpoints before shutdown");
                    }
                    return Ok(Shutdown);
                }
                Either::Right((_, future)) => shutdown_data = future,
            }
            stats.record("sleeping", start.elapsed());
        }
    }

    fn watch_new_file(
        &self,
        path: PathBuf,
        file_id: FileFingerprint,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        checkpoints: &CheckpointsView,
        startup: bool,
        lines: &mut Vec<Line>,
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
        ) {
            Ok(mut watcher) => {
                if let ReadFrom::Checkpoint(file_position) = read_from {
                    self.emitter.emit_file_resumed(&path, file_position);
                } else {
                    self.emitter.emit_file_added(&path);
                }
                // Process any lines read at startup
                let startup_lines = watcher.take_startup_lines();
                if !startup_lines.is_empty() {
                    // Count the original number of lines
                    let original_count = startup_lines.len();

                    // Filter out empty lines to avoid sending empty events
                    let non_empty_lines: Vec<_> = startup_lines
                        .into_iter()
                        .filter(|line| !line.bytes.is_empty())
                        .collect();
                    let filtered_count = non_empty_lines.len();

                    if filtered_count > 0 {
                        debug!(
                            message = "Processing startup lines",
                            original_count = original_count,
                            filtered_count = filtered_count,
                            ?path
                        );

                        for line in non_empty_lines {
                            let bytes_len = line.bytes.len() as u64;
                            lines.push(Line {
                                text: line.bytes,
                                filename: path.to_str().expect("not a valid path").to_owned(),
                                file_id,
                                start_offset: line.offset,
                                end_offset: line.offset + bytes_len,
                            });
                        }
                    } else {
                        debug!(
                            message = "No non-empty startup lines to process",
                            original_count = original_count,
                            ?path
                        );
                    }
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
        tokio::task::spawn_blocking(move || {
            let start = time::Instant::now();
            match checkpointer.write_checkpoints() {
                Ok(count) => emitter.emit_file_checkpointed(count, start.elapsed()),
                Err(error) => emitter.emit_file_checkpoint_write_error(error),
            }
        })
        .await
        .ok();
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
        let total = self.started_at.elapsed();
        let counted: Duration = self.segments.values().sum();
        let other: Duration = self.started_at.elapsed() - counted;
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
