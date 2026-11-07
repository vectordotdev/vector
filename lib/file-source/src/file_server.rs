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
    time::sleep,
};

use tokio_util::task::JoinMap;
use tracing::{debug, error, info, trace};

use crate::{
    file_watcher::{FileWatcher, RawLineResult},
    paths_provider::PathsProvider,
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
    /// `shutdown_data` triggers a final read pass of watched files before stopping
    /// output; `shutdown_checkpointer` stops the background checkpoint writer after
    /// all acknowledgements have completed.
    ///
    /// # Errors
    /// Returns the output sink's error if sending a batch of lines fails.
    ///
    /// # Panics
    /// Panics if the polling deadline overflows, a file path is not UTF-8,
    /// closing the output sink fails, or the checkpoint writer task fails.
    /// Creating a watcher can also panic as described by [`FileWatcher::new`].
    /// Requires a Tokio runtime with timers enabled.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::too_many_lines,
        reason = "Keep the existing polling and shutdown flow together during the lint rollout."
    )]
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
        let mut fp_map: IndexMap<FileFingerprint, FileWatcher> = IndexMap::default();

        let mut backoff_cap: usize = 1;
        let mut lines = Vec::new();

        checkpointer.read_checkpoints(self.ignore_before).await;

        let mut known_small_files = HashMap::new();

        let mut existing_files = Vec::new();
        for path in self.paths_provider.paths() {
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
                .map_or_else(|_| Utc::now(), DateTime::<Utc>::from)
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
        self.emitter.emit_files_open(fp_map.len());

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
        let mut next_glob_time = time::Instant::now();
        let mut shutdown_token = None;
        loop {
            // Glob find files to follow, but not too often.
            let now_time = time::Instant::now();
            if shutdown_token.is_none() && next_glob_time <= now_time {
                // Schedule the next glob time.
                next_glob_time = now_time.checked_add(self.glob_minimum_cooldown).unwrap();

                if stats.started_at.elapsed() > Duration::from_secs(1) {
                    stats.report();
                }

                if stats.started_at.elapsed() > Duration::from_secs(10) {
                    stats = TimingStats::default();
                }

                // Search (glob) for files to detect major file changes.
                let start = time::Instant::now();
                for (_file_id, watcher) in &mut fp_map {
                    watcher.set_file_findable(false); // assume not findable until found
                }
                for path in self.paths_provider.paths() {
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
                                if let Ok(Some(unwatch_info)) = watcher.update_path(path).await {
                                    // Inode changed - emit metrics for the old file
                                    self.emitter.emit_file_unwatched(
                                        &unwatch_info.path,
                                        unwatch_info.reached_eof,
                                        unwatch_info.bytes_unread,
                                    );
                                }
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
                                    if let Ok(Some(unwatch_info)) = watcher.update_path(path).await
                                    {
                                        // Inode changed - emit metrics for the old file
                                        self.emitter.emit_file_unwatched(
                                            &unwatch_info.path,
                                            unwatch_info.reached_eof,
                                            unwatch_info.bytes_unread,
                                        );
                                    }
                                }
                            }
                        } else {
                            // untracked file fingerprint
                            self.watch_new_file(path, file_id, &mut fp_map, &checkpoints, false)
                                .await;
                            self.emitter.emit_files_open(fp_map.len());
                        }
                    }
                }
                stats.record("discovery", start.elapsed());
            }

            // Cleanup the known_small_files
            if let Some(grace_period) = self.remove_after {
                let mut set = JoinMap::new();

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
                // A shutdown pass must also bypass each watcher's EOF backoff.
                if shutdown_token.is_none() && !watcher.should_read() {
                    continue;
                }

                let start = time::Instant::now();
                let mut bytes_read: usize = 0;
                while let Ok(RawLineResult {
                    raw_line: Some(line),
                    discarded_for_size_and_truncated,
                }) = watcher.read_line().await
                {
                    for buf in &discarded_for_size_and_truncated {
                        self.emitter.emit_file_line_too_long(
                            &buf.clone(),
                            self.max_line_bytes,
                            buf.len(),
                        );
                    }

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
                }

                // Do not move on to newer files if we are behind on an older file
                if self.oldest_first && maxed_out_reading_single_file && shutdown_token.is_none() {
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
            // First collect dead file IDs, then process them (get_unwatch_info is async).
            let dead_file_ids: Vec<_> = fp_map
                .iter()
                .filter(|(_, watcher)| watcher.dead())
                .map(|(file_id, _)| *file_id)
                .collect();

            for file_id in dead_file_ids {
                if let Some(watcher) = fp_map.shift_remove(&file_id) {
                    let unwatch_info = watcher.get_unwatch_info().await;
                    self.emitter.emit_file_unwatched(
                        &unwatch_info.path,
                        unwatch_info.reached_eof,
                        unwatch_info.bytes_unread,
                    );
                    checkpoints.set_dead(file_id);
                }
            }
            self.emitter.emit_files_open(fp_map.len());

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

            if shutdown_token.is_some() {
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
                // Keep the shutdown token until output is closed and checkpoints
                // are written, so the caller does not finish shutdown prematurely.
            }

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
            let sleep = async move {
                if backoff > 0 {
                    sleep(Duration::from_millis(backoff as u64)).await;
                }
            };
            futures::pin_mut!(sleep);
            match select(&mut shutdown_data, sleep).await {
                Either::Left((token, _)) => {
                    // Like Datadog Agent's readForever, read before honoring the
                    // stop request. Wake immediately for one bounded pass over
                    // existing watchers rather than losing lines appended during
                    // the idle backoff. Do not discover new files or drain forever.
                    // https://github.com/DataDog/datadog-agent/blob/7.83.1/pkg/logs/tailers/file/tailer.go#L340-L372
                    shutdown_token = Some(token);
                }
                Either::Right(((), _)) => {}
            }
            stats.record("sleeping", start.elapsed());
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
        let read_from = if self.ignore_checkpoints {
            fallback
        } else {
            checkpoints
                .get(file_id)
                .map_or(fallback, ReadFrom::Checkpoint)
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
        }
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
            () = sleep => {},
        }

        let emitter = emitter.clone();
        let checkpointer = Arc::clone(&checkpointer);
        let start = time::Instant::now();
        match checkpointer.write_checkpoints().await {
            Ok(count) => emitter.emit_file_checkpointed(count, start.elapsed()),
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

impl Default for TimingStats {
    fn default() -> Self {
        Self {
            started_at: time::Instant::now(),
            segments: BTreeMap::default(),
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
    use std::{convert::Infallible, io::Write, path::Path};

    use bytes::BytesMut;
    use file_source_common::FingerprintStrategy;
    use futures::{StreamExt, channel::mpsc, sink};

    use super::*;
    use crate::paths_provider::{Glob, MatchOptions};

    #[derive(Clone)]
    struct Events;

    impl FileSourceInternalEvents for Events {
        fn emit_file_added(&self, _: &Path) {}
        fn emit_file_resumed(&self, _: &Path, _: u64) {}
        fn emit_file_watch_error(&self, _: &Path, _: std::io::Error) {}
        fn emit_file_unwatched(&self, _: &Path, _: bool, _: Option<u64>) {}
        fn emit_file_deleted(&self, _: &Path) {}
        fn emit_file_delete_error(&self, _: &Path, _: std::io::Error) {}
        fn emit_file_fingerprint_read_error(&self, _: &Path, _: std::io::Error) {}
        fn emit_file_checkpointed(&self, _: usize, _: Duration) {}
        fn emit_file_checksum_failed(&self, _: &Path) {}
        fn emit_file_checkpoint_write_error(&self, _: std::io::Error) {}
        fn emit_files_open(&self, _: usize) {}
        fn emit_path_globbing_failed(&self, _: &Path, _: &std::io::Error) {}
        fn emit_file_line_too_long(&self, _: &BytesMut, _: usize, _: usize) {}
    }

    #[tokio::test]
    async fn reads_watched_files_once_more_before_shutdown() {
        assert_final_read(false).await;
    }

    #[tokio::test]
    async fn final_read_visits_all_files_with_oldest_first() {
        assert_final_read(true).await;
    }

    async fn assert_final_read(oldest_first: bool) {
        let dir = tempfile::tempdir().unwrap();
        let paths = [dir.path().join("a.log"), dir.path().join("b.log")];
        for (i, path) in paths.iter().enumerate() {
            std::fs::write(path, format!("tick-{i}\n")).unwrap();
        }
        let server = FileServer {
            paths_provider: Glob::new(&paths, &[], MatchOptions::default(), Events).unwrap(),
            // One line exceeds this budget: the final pass must visit both files,
            // but must not drain the backlog indefinitely.
            max_read_bytes: 1,
            ignore_checkpoints: false,
            read_from: ReadFrom::Beginning,
            ignore_before: None,
            max_line_bytes: 1024,
            line_delimiter: Bytes::from_static(b"\n"),
            data_dir: dir.path().to_owned(),
            glob_minimum_cooldown: Duration::from_mins(1),
            fingerprinter: Fingerprinter::new(
                FingerprintStrategy::FirstLinesChecksum {
                    ignored_header_bytes: 0,
                    lines: 1,
                },
                1024,
                false,
            ),
            oldest_first,
            remove_after: None,
            emitter: Events,
            rotate_wait: Duration::from_mins(1),
        };
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let (tx, rx) = mpsc::unbounded();
        // Append only once the file has reached EOF, immediately before the idle
        // wait. Making shutdown ready here avoids a timing-dependent sleep race.
        let output = sink::unfold(
            (tx, Some(shutdown_tx)),
            move |(tx, mut shutdown), lines: Vec<Line>| {
                if lines.is_empty()
                    && let Some(shutdown) = shutdown.take()
                {
                    for (i, path) in paths.iter().enumerate() {
                        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
                        writeln!(file, "FINAL-{i}\nbacklog").unwrap();
                    }
                    shutdown.send(()).unwrap();
                }
                for line in lines {
                    tx.unbounded_send(line.text).unwrap();
                }
                futures::future::ready(Ok::<_, Infallible>((tx, shutdown)))
            },
        );
        let checkpointer = Checkpointer::new(dir.path());
        tokio::time::timeout(
            Duration::from_secs(5),
            server.run(
                Box::pin(output),
                shutdown_rx,
                futures::future::ready(()),
                checkpointer,
            ),
        )
        .await
        .expect("file server did not shut down")
        .unwrap();
        let mut lines: Vec<_> = rx.collect().await;
        lines.sort();
        assert_eq!(
            lines,
            ["FINAL-0", "FINAL-1", "tick-0", "tick-1"].map(Bytes::from)
        );
    }
}
