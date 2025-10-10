use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    time::Instant,
    time::{self, Duration},
};

#[cfg(any(test, feature = "test"))]
use bytes::Buf;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{Future, Sink, SinkExt};
use indexmap::IndexMap;
#[cfg(any(test, feature = "test"))]
use tokio::sync::mpsc;
use tokio::{
    fs::{self, remove_file},
    time::timeout,
};
use tracing::{debug, error, info, trace};

use crate::{
    file_watcher::FileWatcher, paths_provider::PathsProvider, Checkpointer, CheckpointsView,
    FilePosition, ReadFrom,
};
use file_source_common::{
    internal_events::FileSourceExtendedInternalEvents as FileSourceInternalEvents, FileFingerprint,
    Fingerprinter, TaskSet,
};

#[cfg(any(test, feature = "test"))]
#[derive(Debug, Clone)]
pub enum TestEvent {
    Checkpointed(PathBuf),
    Read(PathBuf, Box<[u8]>),
}

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
    pub fingerprinter: Fingerprinter,
    pub oldest_first: bool,
    pub remove_after: Option<Duration>,
    pub emitter: E,
    pub rotate_wait: Duration,
    /// Duration after which to checkpoint files
    pub checkpoint_interval: Duration,
    #[cfg(any(test, feature = "test"))]
    pub test_sender: Option<mpsc::UnboundedSender<TestEvent>>,
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
        // We're using notify-based watching for all files
        // This means we only need to glob once on startup to discover files
        // that were added while Vector was stopped
        debug!(message = "Using notify-based watching for all files");

        let mut fp_map: IndexMap<FileFingerprint, FileWatcher> = Default::default();

        // We no longer need backoff_cap since we use a fixed backoff
        let mut lines = Vec::new();

        checkpointer.read_checkpoints(self.ignore_before).await;

        let mut known_small_files = HashMap::new();

        let mut existing_files = Vec::new();

        let paths = self.paths_provider.paths(true).await;
        for path in paths.into_iter() {
            debug!(?path, "fingerprinting on startup");
            if let Some(file_id) = self
                .fingerprinter
                .fingerprint_or_emit(&path, &mut known_small_files, &self.emitter)
                .await
            {
                existing_files.push((path, file_id));
            }
        }

        let mut metadata_set = TaskSet::new();
        existing_files.iter().for_each(|(path, _file_id)| {
            metadata_set.spawn(path.clone(), fs::metadata(path.clone()))
        });

        let mut created_map: HashMap<PathBuf, DateTime<Utc>> = Default::default();
        let now = Utc::now(); // This could be a local OnceCell

        while let Some((path, result)) = metadata_set.join_next().await {
            match result.map_err(std::io::Error::other).flatten() {
                Ok(metadata) => {
                    created_map.insert(
                        path,
                        metadata.created().map(DateTime::<Utc>::from).unwrap_or(now),
                    );
                }
                Err(_err) => {
                    // TODO: log error
                    created_map.insert(path, now);
                }
            }
        }

        existing_files.sort_by(|(path_a, _), (path_b, _)| {
            let a = created_map.get(path_a).unwrap_or(&now);
            let b = created_map.get(path_b).unwrap_or(&now);
            a.cmp(b)
        });

        let checkpoints = checkpointer.view();

        debug!(?existing_files);
        for (path, file_id) in existing_files {
            // TODO parallelize?
            self.watch_new_file(
                path.clone(),
                file_id,
                &mut fp_map,
                &checkpoints,
                true,
                &mut lines,
            )
            .await;

            #[cfg(any(test, feature = "test"))]
            if let Some(sender) = self.test_sender.as_ref() {
                sender.send(TestEvent::Checkpointed(path.clone())).unwrap();
            }
        }
        self.emitter.emit_files_open(fp_map.len());

        let mut stats = TimingStats::default();

        // Spawn the checkpoint writer task with the configured interval
        // This ensures that checkpoints are written periodically to disk
        let checkpoint_interval = self.checkpoint_interval;

        let checkpoint_task_handle = tokio::spawn(checkpoint_writer(
            checkpointer,
            checkpoint_interval,
            shutdown_checkpointer,
            self.emitter.clone(),
        ));

        let mut last_stats_report: Option<Instant> = None;
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
            // Check for new files frequently to minimize the delay between when a file is discovered
            // by the notify watcher and when it's actually processed, but not on every iteration
            // to avoid excessive CPU usage
            let should_discover_glob = next_glob_time <= now_time
                || now_time.duration_since(next_glob_time) > Duration::from_millis(100);

            // Report stats periodically, but only if enough time has passed since the last report
            // This prevents excessive logging when the main loop is running frequently
            let now = Instant::now();
            let should_report_stats = {
                if let Some(last_report) = last_stats_report {
                    if now.duration_since(last_report) >= Duration::from_secs(10) {
                        last_stats_report = Some(now);
                        true
                    } else {
                        false
                    }
                } else {
                    last_stats_report = Some(now);
                    true
                }
            };

            if should_report_stats && stats.started_at.elapsed() > Duration::from_secs(10) {
                stats.report();
            }

            if stats.started_at.elapsed() > Duration::from_secs(10) {
                stats = TimingStats::default();
            }

            if should_discover_glob {
                // Schedule the next glob time - use a fixed interval of 1 second
                next_glob_time = now_time.checked_add(Duration::from_secs(1)).unwrap();
            }

            // Search for files to detect major file changes.
            let start = time::Instant::now();
            for (_file_id, watcher) in &mut fp_map {
                watcher.set_file_findable(false); // assume not findable until found
            }

            // Use async paths provider
            let paths = self.paths_provider.paths(should_discover_glob).await;
            for path in paths.into_iter() {
                if let Some(file_id) = self
                    .fingerprinter
                    .fingerprint_or_emit(&path, &mut known_small_files, &self.emitter)
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
                            ) {
                                if old_modified_time < new_modified_time {
                                    info!(
                                        message = "Switching to watch most recently modified file.",
                                        new_modified_time = ?new_modified_time,
                                        old_modified_time = ?old_modified_time,
                                    );
                                    watcher.update_path(path).await.ok(); // ok if this fails: might fix next cycle
                                }
                            }
                        }
                    } else {
                        // untracked file fingerprint
                        // Immediately watch and read the new file
                        debug!(message = "Discovered new file during runtime", ?path);
                        self.watch_new_file(
                            path.clone(),
                            file_id,
                            &mut fp_map,
                            &checkpoints,
                            false,
                            &mut lines,
                        )
                        .await;

                        // Immediately read the file to avoid delay in detecting content
                        if let Some(watcher) = fp_map.get_mut(&file_id) {
                            debug!(message = "Immediately reading newly discovered file", ?path);
                            let mut bytes_read: usize = 0;
                            while let Ok(Some(line)) = watcher.read_line().await {
                                let sz = line.bytes.len();
                                trace!(message = "Read bytes from new file", ?path, bytes = ?sz);
                                bytes_read += sz;

                                lines.push(Line {
                                    text: line.bytes,
                                    filename: watcher
                                        .path
                                        .to_str()
                                        .expect("not a valid path")
                                        .to_owned(),
                                    file_id,
                                    start_offset: line.offset,
                                    end_offset: watcher.get_file_position(),
                                });
                            }

                            if bytes_read > 0 {
                                debug!(
                                    message = "Read initial content from newly discovered file",
                                    ?path,
                                    bytes = bytes_read
                                );
                            }
                        }
                        self.emitter.emit_files_open(fp_map.len());
                    }
                }
            }
            stats.record("discovery", start.elapsed());

            // Cleanup the known_small_files
            if let Some(grace_period) = self.remove_after {
                let mut set = TaskSet::new();

                known_small_files
                    .iter()
                    .filter(|&(_path, last_time_open)| last_time_open.elapsed() >= grace_period)
                    .map(|(path, _last_time_open)| path.clone())
                    .for_each(|path| set.spawn(path.clone(), remove_file(path)));

                while let Some((path, result)) = set.join_next().await {
                    match result.map_err(std::io::Error::other).flatten() {
                        Ok(()) => {
                            let removed = known_small_files.remove(&path);

                            if removed.is_some() {
                                self.emitter.emit_file_deleted(&path);
                            }
                        }
                        Err(err) => {
                            self.emitter.emit_file_delete_error(&path, err);
                        }
                    }
                }
            }

            // Collect lines by polling files.
            let mut global_bytes_read: usize = 0;
            let mut maxed_out_reading_single_file = false;
            for (&file_id, watcher) in &mut fp_map {
                let start = time::Instant::now();
                let mut bytes_read: usize = 0;
                while let Ok(Some(line)) = watcher.read_line().await {
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
                if watcher.reached_eof()
                    && watcher.last_read_success().elapsed() < Duration::from_secs(1)
                {
                    // Update the notify watcher with the current position
                    if let Err(e) = watcher.update_watcher().await {
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

            #[cfg(any(test, feature = "test"))]
            if let Some(sender) = self.test_sender.as_ref() {
                for line in &lines {
                    debug!("sending {}", String::from_utf8_lossy(&line.text.chunk()));
                    sender
                        .send(TestEvent::Read(
                            PathBuf::from(line.filename.clone()),
                            line.text.chunk().into(),
                        ))
                        .unwrap();
                }
            }
            let start = time::Instant::now();
            let to_send = std::mem::take(&mut lines);
            match chans.send(to_send).await {
                Ok(()) => {}
                Err(error) => {
                    error!(message = "Output channel closed.", %error);
                    return Err(error);
                }
            }
            stats.record("sending", start.elapsed());

            if futures::poll!(&mut shutdown_data).is_ready() {
                // Shut down all file watchers to prevent further events
                debug!(
                    message = "Shutting down all file watchers",
                    count = fp_map.len()
                );
                // This will drop all watchers. See `FileWatcher::drop`
                fp_map.retain(|_, _| false);

                chans
                    .close()
                    .await
                    .expect("error closing file_server data channel");
                checkpoint_task_handle
                    .await
                    .expect("checkpoint task has was cancelled or panicked");
                return Ok(Shutdown);
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
        )
        .await
        {
            Ok((mut watcher, startup_lines)) => {
                if let ReadFrom::Checkpoint(file_position) = read_from {
                    self.emitter.emit_file_resumed(&path, file_position);
                } else {
                    self.emitter.emit_file_added(&path);

                    #[cfg(any(test, feature = "test"))]
                    if let Some(sender) = self.test_sender.as_ref() {
                        debug!("watching {path:?}");
                        sender.send(TestEvent::Checkpointed(path.clone())).unwrap();
                    }
                }

                // Process any lines read at startup
                if !startup_lines.is_empty() {
                    // Process all startup lines, including empty ones
                    let line_count = startup_lines.len();
                    if line_count > 0 {
                        debug!(
                            message = "Processing startup lines",
                            line_count = line_count,
                            ?path
                        );

                        // Add the lines to the output
                        for line in startup_lines {
                            let bytes_len = line.bytes.len() as u64;
                            lines.push(Line {
                                text: line.bytes,
                                filename: path.to_string_lossy().into_owned(),
                                file_id,
                                start_offset: line.offset,
                                end_offset: line.offset + bytes_len,
                            });
                        }
                    }
                }

                watcher.set_file_findable(true);
                fp_map.insert(file_id, watcher);
            }
            Err(error) => self.emitter.emit_file_watch_error(&path, error),
        };
    }
}

/// Write checkpoints to file, sleeping `sleep_duration` in between writes
async fn checkpoint_writer(
    checkpointer: Checkpointer,
    sleep_duration: Duration,
    mut shutdown: impl Future + Unpin,
    emitter: impl FileSourceInternalEvents,
) -> Checkpointer {
    let mut should_shutdown = false;
    while !should_shutdown {
        should_shutdown = timeout(sleep_duration, &mut shutdown).await.is_ok();
        if should_shutdown {
            debug!("Writing checkpoints before shutdown");
        }

        let emitter = emitter.clone();
        let start = time::Instant::now();
        match checkpointer.write_checkpoints().await {
            Ok(count) => emitter.emit_file_checkpointed(count, start.elapsed()),
            Err(error) => {
                if should_shutdown {
                    error!(?error, "Error writing checkpoints before shutdown");
                }
                emitter.emit_file_checkpoint_write_error(error);
            }
        }
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
    pub start_offset: FilePosition,
    pub end_offset: FilePosition,
}
