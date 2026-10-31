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
    fs::{self, File, remove_file},
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

        let paths: Vec<_> = self.paths_provider.paths().into_iter().collect();
        let metadata = join_all(paths.iter().map(fs::metadata)).await;

        let created = metadata.into_iter().map(|m| {
            m.and_then(|m| m.created())
                .map_or_else(|_| Utc::now(), DateTime::<Utc>::from)
        });

        let mut existing_files: Vec<(DateTime<Utc>, PathBuf)> = paths
            .into_iter()
            .zip(created)
            .map(|(path, key)| (key, path))
            .collect();

        existing_files.sort_by_key(|(key, _)| *key);

        let checkpoints = checkpointer.view();

        for (_key, path) in existing_files {
            if let Some((file_id, file)) = self
                .fingerprinter
                .fingerprint_or_emit(&path, &mut known_small_files, &self.emitter)
                .await
            {
                self.watch_new_file(path, file_id, file, &mut fp_map, &checkpoints, true)
                    .await;
            }
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
        loop {
            // Glob find files to follow, but not too often.
            let now_time = time::Instant::now();
            if next_glob_time <= now_time {
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
                    if let Some((file_id, file)) = self
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
                                if let Ok(Some(unwatch_info)) =
                                    watcher.update_path(path, file).await
                                {
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
                                let old_path = &watcher.path;
                                if let (Ok(old_modified_time), Ok(new_modified_time)) = (
                                    fs::metadata(old_path).await.and_then(|m| m.modified()),
                                    file.metadata().await.and_then(|m| m.modified()),
                                ) && old_modified_time < new_modified_time
                                {
                                    info!(
                                        message = "Switching to watch most recently modified file.",
                                        new_modified_time = ?new_modified_time,
                                        old_modified_time = ?old_modified_time,
                                    );
                                    if let Ok(Some(unwatch_info)) =
                                        watcher.update_path(path, file).await
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
                            self.watch_new_file(
                                path,
                                file_id,
                                file,
                                &mut fp_map,
                                &checkpoints,
                                false,
                            )
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
                        match watcher.remove_file().await {
                            Ok(true) => {
                                self.emitter.emit_file_deleted(&watcher.path);
                                watcher.set_dead();
                            }
                            Ok(false) => {}
                            Err(error) => {
                                // We will try again after some time.
                                self.emitter.emit_file_delete_error(&watcher.path, error);
                            }
                        }
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
            stats.record("sleeping", start.elapsed());
        }
    }

    async fn watch_new_file(
        &self,
        path: PathBuf,
        file_id: FileFingerprint,
        file: File,
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
            file,
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
    use std::{
        fs as std_fs, io,
        path::Path,
        sync::atomic::{AtomicBool, Ordering},
    };

    use async_compression::tokio::bufread::GzipEncoder;
    use file_source_common::FingerprintStrategy;
    use futures::{
        FutureExt as _, StreamExt as _,
        channel::{mpsc, oneshot},
    };
    use tokio::{fs::File, io::AsyncReadExt as _};

    use crate::file_watcher::RawLine;

    use super::*;

    #[derive(Clone, Default)]
    struct NoErrors {
        rotate_on_add: Option<(PathBuf, &'static [u8])>,
    }

    impl FileSourceInternalEvents for NoErrors {
        fn emit_file_added(&self, path: &Path) {
            if let Some((rotated, replacement)) = &self.rotate_on_add {
                std_fs::rename(path, rotated).unwrap();
                std_fs::write(path, replacement).unwrap();
            }
        }
        fn emit_file_resumed(&self, _: &Path, _: u64) {}
        fn emit_file_watch_error(&self, path: &Path, error: io::Error) {
            panic!("watch {path:?}: {error}");
        }
        fn emit_file_unwatched(&self, _: &Path, _: bool, _: Option<u64>) {}
        fn emit_file_deleted(&self, _: &Path) {}
        fn emit_file_delete_error(&self, path: &Path, error: io::Error) {
            panic!("delete {path:?}: {error}");
        }
        fn emit_file_fingerprint_read_error(&self, path: &Path, error: io::Error) {
            panic!("fingerprint {path:?}: {error}");
        }
        fn emit_file_checkpointed(&self, _: usize, _: Duration) {}
        fn emit_file_checksum_failed(&self, path: &Path) {
            panic!("checksum {path:?}");
        }
        fn emit_file_checkpoint_write_error(&self, error: io::Error) {
            panic!("checkpoint: {error}");
        }
        fn emit_files_open(&self, _: usize) {}
        fn emit_path_globbing_failed(&self, path: &Path, error: &io::Error) {
            panic!("glob {path:?}: {error}");
        }
        fn emit_file_line_too_long(&self, _: &bytes::BytesMut, _: usize, _: usize) {
            panic!("unexpected oversized line");
        }
    }

    struct NoPaths;

    impl PathsProvider for NoPaths {
        type IntoIter = Vec<PathBuf>;

        fn paths(&self) -> Self::IntoIter {
            Vec::new()
        }
    }

    struct DiscoverAfterStartup {
        path: PathBuf,
        startup: AtomicBool,
    }

    impl PathsProvider for DiscoverAfterStartup {
        type IntoIter = Vec<PathBuf>;

        fn paths(&self) -> Self::IntoIter {
            if self.startup.swap(false, Ordering::Relaxed) {
                Vec::new()
            } else {
                vec![self.path.clone()]
            }
        }
    }

    fn checksum_strategy() -> FingerprintStrategy {
        FingerprintStrategy::FirstLinesChecksum {
            ignored_header_bytes: 0,
            lines: 1,
        }
    }

    fn test_server(
        data_dir: &Path,
        strategy: FingerprintStrategy,
    ) -> FileServer<NoPaths, NoErrors> {
        test_server_with_paths(data_dir, strategy, NoPaths)
    }

    fn test_server_with_paths<PP: PathsProvider>(
        data_dir: &Path,
        strategy: FingerprintStrategy,
        paths_provider: PP,
    ) -> FileServer<PP, NoErrors> {
        FileServer {
            paths_provider,
            max_read_bytes: 2048,
            ignore_checkpoints: false,
            read_from: ReadFrom::Beginning,
            ignore_before: None,
            max_line_bytes: 1024,
            line_delimiter: Bytes::from_static(b"\n"),
            data_dir: data_dir.to_path_buf(),
            glob_minimum_cooldown: Duration::from_millis(25),
            fingerprinter: Fingerprinter::new(strategy, 1024, false),
            oldest_first: false,
            remove_after: None,
            emitter: NoErrors::default(),
            rotate_wait: Duration::from_mins(1),
        }
    }

    async fn fingerprint(
        server: &mut FileServer<NoPaths, NoErrors>,
        path: &Path,
    ) -> (FileFingerprint, File) {
        server
            .fingerprinter
            .fingerprint_or_emit(path, &mut HashMap::new(), &server.emitter)
            .await
            .unwrap()
    }

    async fn next_line(watcher: &mut FileWatcher) -> RawLine {
        watcher.read_line().await.unwrap().raw_line.unwrap()
    }

    #[tokio::test]
    async fn remove_after_preserves_replacement_file() {
        const ORIGINAL: &[u8] = b"original event\n";
        const REPLACEMENT: &[u8] = b"unread replacement event\n";

        for rotate in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("0.log");
            let rotated = dir.path().join("0.log.rotated");
            std_fs::write(&path, ORIGINAL).unwrap();
            let paths = DiscoverAfterStartup {
                path: path.clone(),
                startup: AtomicBool::new(true),
            };
            let mut server = test_server_with_paths(dir.path(), checksum_strategy(), paths);
            server.glob_minimum_cooldown = Duration::from_mins(1);
            server.remove_after = Some(Duration::ZERO);
            if rotate {
                server.emitter.rotate_on_add = Some((rotated.clone(), REPLACEMENT));
            }
            let (tx, mut rx) = mpsc::channel(1);
            let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
            let shutdown = shutdown_rx.shared();
            let run = server.run(
                tx,
                shutdown.clone(),
                shutdown,
                Checkpointer::new(dir.path()),
            );
            let consume = async {
                let batch = rx.next().await.unwrap();
                assert_eq!(batch.len(), 1);
                assert_eq!(batch[0].text.as_ref(), &ORIGINAL[..ORIGINAL.len() - 1]);
                // Two idle batches allow the EOF read backoff to expire before cleanup.
                for _ in 0..2 {
                    assert!(rx.next().await.unwrap().is_empty());
                }
                if rotate {
                    assert_eq!(std_fs::read(&path).unwrap(), REPLACEMENT);
                    assert_eq!(std_fs::read(&rotated).unwrap(), ORIGINAL);
                } else {
                    assert!(!path.exists());
                }
                shutdown_tx.send(()).unwrap();
                while rx.next().await.is_some() {}
            };
            tokio::time::timeout(Duration::from_secs(5), async {
                let (result, ()) = futures::future::join(run, consume).await;
                result.unwrap();
            })
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn rotation_uses_fingerprinted_handle_and_checkpoint() {
        const FIRST: &[u8] = b"2026-10-16T00:31:00.000Z stdout F first event\n";
        const TARGET: &[u8] = b"2026-10-16T00:32:52.539Z stdout F original event\n";
        const REPLACEMENT: &[u8] = b"2026-10-16T00:33:00.000Z stdout F replacement event\n";

        for strategy in [checksum_strategy(), FingerprintStrategy::DevInode] {
            for resume in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("0.log");
                let rotated = dir.path().join("0.log.rotated");
                let mut original = FIRST.to_vec();
                original.extend_from_slice(TARGET);
                std_fs::write(&path, &original).unwrap();
                let mut server = test_server(dir.path(), strategy.clone());
                let (original_id, original_file) = fingerprint(&mut server, &path).await;
                let checkpointer = Checkpointer::new(dir.path());
                let checkpoints = checkpointer.view();
                if resume {
                    checkpoints.update(original_id, u64::try_from(FIRST.len()).unwrap());
                }

                std_fs::rename(&path, &rotated).unwrap();
                std_fs::write(&path, REPLACEMENT).unwrap();
                let mut watchers = IndexMap::new();
                server
                    .watch_new_file(
                        path.clone(),
                        original_id,
                        original_file,
                        &mut watchers,
                        &checkpoints,
                        false,
                    )
                    .await;
                let original_watcher = watchers.get_mut(&original_id).unwrap();
                if !resume {
                    let line = next_line(original_watcher).await;
                    assert_eq!(line.bytes.as_ref(), &FIRST[..FIRST.len() - 1]);
                    assert_eq!(line.offset, 0);
                }
                let line = next_line(original_watcher).await;
                assert_eq!(line.bytes.as_ref(), &TARGET[..TARGET.len() - 1]);
                assert_eq!(line.offset, u64::try_from(FIRST.len()).unwrap());
                assert_eq!(
                    original_watcher.get_file_position(),
                    u64::try_from(original.len()).unwrap()
                );

                let (replacement_id, replacement_file) = fingerprint(&mut server, &path).await;
                assert_ne!(replacement_id, original_id);
                server
                    .watch_new_file(
                        path,
                        replacement_id,
                        replacement_file,
                        &mut watchers,
                        &checkpoints,
                        false,
                    )
                    .await;
                let line = next_line(watchers.get_mut(&replacement_id).unwrap()).await;
                assert_eq!(line.bytes.as_ref(), &REPLACEMENT[..REPLACEMENT.len() - 1]);
                assert_eq!(line.offset, 0);
                assert_eq!(watchers.len(), 2);
                assert_eq!(std_fs::read(rotated).unwrap(), original);
            }
        }
    }

    #[tokio::test]
    async fn replacement_uses_fingerprinted_handle_and_preserves_offset() {
        const CONTENT: &[u8] = b"same first line\nsecond line\nthird line\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.log");
        let candidate = dir.path().join("candidate.log");
        let renamed = dir.path().join("candidate.log.rotated");
        std_fs::write(&path, CONTENT).unwrap();
        std_fs::write(&candidate, CONTENT).unwrap();
        let mut server = test_server(dir.path(), checksum_strategy());
        let (original_id, original_file) = fingerprint(&mut server, &path).await;
        let mut watcher = FileWatcher::new(
            path.clone(),
            original_file,
            ReadFrom::Beginning,
            None,
            1024,
            Bytes::from_static(b"\n"),
        )
        .await
        .unwrap();
        let first = next_line(&mut watcher).await;
        assert_eq!(first.bytes.as_ref(), b"same first line");
        let position = watcher.get_file_position();
        let (candidate_id, candidate_file) = fingerprint(&mut server, &candidate).await;
        assert_eq!(candidate_id, original_id);

        std_fs::rename(&candidate, &renamed).unwrap();
        std_fs::write(&candidate, b"unrelated replacement\n").unwrap();
        let old_info = watcher
            .update_path(candidate, candidate_file)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(old_info.path, path);
        assert_eq!(watcher.get_file_position(), position);
        for expected in [b"second line".as_slice(), b"third line".as_slice()] {
            let line = next_line(&mut watcher).await;
            assert_eq!(line.bytes.as_ref(), expected);
        }
        assert!(watcher.read_line().await.unwrap().raw_line.is_none());
        assert_eq!(std_fs::read(renamed).unwrap(), CONTENT);
    }

    #[tokio::test]
    async fn fingerprinted_gzip_rewinds_and_keeps_skip_semantics() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.log.gz");
        let mut compressed = Vec::new();
        GzipEncoder::new(b"first\nsecond\n".as_slice())
            .read_to_end(&mut compressed)
            .await
            .unwrap();
        std_fs::write(&path, compressed).unwrap();
        let mut server = test_server(dir.path(), checksum_strategy());
        for read_from in [ReadFrom::Beginning, ReadFrom::End, ReadFrom::Checkpoint(6)] {
            let (_, file) = fingerprint(&mut server, &path).await;
            let mut watcher = FileWatcher::new(
                path.clone(),
                file,
                read_from,
                None,
                1024,
                Bytes::from_static(b"\n"),
            )
            .await
            .unwrap();
            if read_from == ReadFrom::Beginning {
                for expected in [b"first".as_slice(), b"second".as_slice()] {
                    let line = next_line(&mut watcher).await;
                    assert_eq!(line.bytes.as_ref(), expected);
                }
            }
            assert!(watcher.read_line().await.unwrap().raw_line.is_none());
        }
    }

    #[tokio::test]
    async fn fingerprinted_handle_preserves_start_position_options() {
        let cases = [
            ("startup/end", true, ReadFrom::End, false, None, 2),
            ("new/end", false, ReadFrom::End, false, None, 0),
            ("startup/start", true, ReadFrom::Beginning, false, None, 0),
            ("checkpoint/end", true, ReadFrom::End, false, Some(6), 1),
            ("checkpoint/new", false, ReadFrom::End, false, Some(6), 1),
            ("ignored/end", true, ReadFrom::End, true, Some(6), 2),
            ("ignored/new", false, ReadFrom::End, true, Some(6), 0),
            ("ignored/start", true, ReadFrom::Beginning, true, Some(6), 0),
        ];
        let lines = [b"first".as_slice(), b"second".as_slice()];
        for (name, startup, read_from, ignore_checkpoints, checkpoint, skip) in cases {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("input.log");
            std_fs::write(&path, b"first\nsecond\n").unwrap();
            let mut server = test_server(dir.path(), checksum_strategy());
            server.read_from = read_from;
            server.ignore_checkpoints = ignore_checkpoints;
            let (file_id, file) = fingerprint(&mut server, &path).await;
            let checkpointer = Checkpointer::new(dir.path());
            let checkpoints = checkpointer.view();
            if let Some(position) = checkpoint {
                checkpoints.update(file_id, position);
            }
            let mut watchers = IndexMap::new();
            server
                .watch_new_file(path, file_id, file, &mut watchers, &checkpoints, startup)
                .await;
            let watcher = watchers.get_mut(&file_id).unwrap();
            let mut position = u64::try_from(
                lines[..skip]
                    .iter()
                    .map(|line| line.len() + 1)
                    .sum::<usize>(),
            )
            .unwrap();
            assert_eq!(watcher.get_file_position(), position, "{name}");
            for expected in &lines[skip..] {
                let line = next_line(watcher).await;
                assert_eq!(line.bytes.as_ref(), *expected, "{name}");
                assert_eq!(line.offset, position, "{name}");
                position += u64::try_from(expected.len() + 1).unwrap();
            }
            assert!(
                watcher.read_line().await.unwrap().raw_line.is_none(),
                "{name}"
            );
        }
    }
}
