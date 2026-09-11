use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use std::{
    collections::{HashMap, HashSet},
    io::{self, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use tokio::{
    fs::File,
    io::{AsyncBufRead, AsyncBufReadExt, AsyncSeekExt, BufReader},
    time::Instant,
};
use tracing::debug;
use vector_common::constants::GZIP_MAGIC;

use file_source_common::{
    AsyncFileInfo, FilePosition, PortableFileExt, ReadFrom,
    buffer::{ReadResult, read_until_with_max_size},
};
use vector_common::compression::gzip_multiple_decoder;

const EOF_READ_BACKOFF_MIN: Duration = Duration::from_millis(1);
const EOF_READ_BACKOFF_MAX: Duration = Duration::from_millis(250);

#[cfg(test)]
mod tests;

/// The `RawLine` struct is a thin wrapper around the bytes that have been read
/// in order to retain the context of where in the file they have been read from.
///
/// The offset field contains the byte offset of the beginning of the line within
/// the file that it was read from.
#[derive(Debug)]
pub struct RawLine {
    pub offset: u64,
    pub bytes: Bytes,
}

#[derive(Debug)]
pub struct RawLineResult {
    pub raw_line: Option<RawLine>,
    pub discarded_for_size_and_truncated: Vec<BytesMut>,
}

/// The read-oriented state of a [`FileWatcher`].
///
/// `Active` is the traditional, always-has-been state: an open file handle is
/// held and reads are attempted against it.
///
/// `Idle` is new: no file handle is held at all. This is used both for files
/// which are old/fully-read at discovery time (so we never have to open them)
/// and for files which used to be `Active` but have gone quiet (reached EOF
/// and had no new writes for `idle_timeout`). While `Idle`, the watcher is
/// polled cheaply via `fs::metadata` (no `File::open`) to detect growth,
/// truncation, or deletion, and is transparently promoted back to `Active`
/// (reopening the file and seeking to `file_position`) when new data shows up.
enum WatcherState {
    Active {
        reader: Box<dyn AsyncBufRead + Send + Unpin>,
        reached_eof: bool,
        last_read_attempt: Instant,
        last_read_success: Instant,
        read_retry_delay: Duration,
        buf: BytesMut,
    },
    Idle {
        /// Last known size of the file, as of the last successful stat. `None` means the stat
        /// taken while entering `Idle` failed, so no raw-size baseline is available yet.
        last_known_size: Option<u64>,
        /// Last known mtime of the file, as of the last successful stat. Used,
        /// together with `last_known_size`, to cheaply detect whether the file
        /// has been written to (or truncated) since we last looked, without
        /// opening it.
        last_known_mtime: Option<SystemTime>,
        /// The time from which this watcher's current idle streak should be measured: either the
        /// last successful read before `deactivate` closed the handle (not the later moment
        /// `deactivate` itself ran, which would double-count `idle_timeout`), or the time
        /// `check_for_new_data` most recently observed a change while already `Idle`. Used by
        /// `FileServer` to drive `remove_after`-style grace-period cleanup for idle files, since
        /// idle watchers never perform reads and so can't rely on "time since last successful
        /// read" the way `Active` watchers do.
        idle_since: Instant,
        /// Set once `check_for_new_data` ever observes the file shrink while `Idle`, and never
        /// cleared until the next `deactivate()` starts a fresh `Idle` period. `reactivate`'s own
        /// point-in-time size check (current size vs. `file_position`) alone isn't enough: a
        /// truncate followed by a fast refill *past* the old `file_position` (e.g. read up to
        /// 1000, truncated to 0, then filled back past 1000 with new content, all before the next
        /// poll) looks, at the moment of reactivation, exactly like ordinary growth -- the
        /// current size is >= `file_position`, so nothing about that single comparison reveals
        /// that a truncation happened in between. Remembering that *some* poll along the way saw
        /// a shrink, even if the file has since grown past the old position again, is what lets
        /// `reactivate` still reset to 0 in that case instead of seeking into what looks like
        /// "the same file, just grown" but is actually unrelated new content sharing old bytes'
        /// former offsets.
        truncated_while_idle: bool,
        /// Set by `invalidate_idle_bookkeeping` to force the next `check_for_new_data` call to
        /// report `changed`, regardless of what it actually observes -- see that function's doc
        /// comment for why. Deliberately a separate flag rather than clobbering
        /// `last_known_size`/`last_known_mtime` with an impossible sentinel (an earlier version of
        /// this did that): doing so destroys the one real baseline `check_for_new_data`'s own
        /// shrink detection needs, so a genuine truncation occurring *after* the sentinel was set
        /// but *before* the next poll would go completely undetected -- not just fail to latch
        /// `truncated_while_idle`, but be invisible to the size comparison entirely, since there's
        /// no longer a real "last known size" for the new, smaller size to compare against.
        /// Keeping the real baseline intact and layering this flag on top lets `check_for_new_data`
        /// still correctly detect a real shrink on the very poll that also honors the forced
        /// retry, instead of having to choose between the two.
        force_recheck: bool,
        /// A line that was buffered but never saw its delimiter before `deactivate` closed the
        /// handle, kept (with its starting offset) so `take_final_partial_line` can salvage it if
        /// this watcher is reaped while still `Idle`, without ever reactivating. Ignored by
        /// `reactivate` itself: `deactivate` already rewinds `file_position` behind these bytes,
        /// so a successful reactivation just re-reads them from disk.
        pending_partial_line: Option<(FilePosition, Bytes)>,
        /// Whether this watcher had reached EOF as of going `Idle`. `deactivate` only ever runs
        /// after `reached_eof()` was already `true`, and the startup fast path in
        /// `FileWatcher::new` starts a fully-read file `Idle` directly, so this is `true` in both
        /// cases that produce an `Idle` watcher. Kept so `reached_eof()` (used by
        /// `FileServer`'s `emit_file_unwatched` telemetry) doesn't misreport an `Idle` watcher as
        /// having been abandoned mid-file, since `Active`'s own `reached_eof` flag would otherwise
        /// be lost the moment the state switches.
        reached_eof: bool,
    },
}

/// The `FileWatcher` struct defines the polling based state machine which reads
/// from a file path, transparently updating the underlying file descriptor when
/// the file has been rolled over, as is common for logs.
///
/// The `FileWatcher` is expected to live for the lifetime of the file
/// path. `FileServer` is responsible for clearing away `FileWatchers` which no
/// longer exist.
pub struct FileWatcher {
    pub path: PathBuf,
    /// Canonical form of `path`, cached when the watcher is created or moved. Notify backends
    /// may report either this form or the logical path used by the glob.
    canonical_path: Option<PathBuf>,
    findable: bool,
    /// Set when an idle watcher was located by its identity at a path outside the configured
    /// include patterns. Such a watcher must continue to be polled there after rotation, even
    /// though the normal glob pass will keep marking it unfindable.
    path_outside_glob: bool,
    state: WatcherState,
    file_position: FilePosition,
    /// Device and inode of the underlying file, once known. The startup idle path obtains this
    /// from its short-lived gzip probe, while `None` remains a safe fallback if that probe cannot
    /// complete.
    /// Callers that need identity to detect renames (`update_path`) already
    /// treat "identity unknown" the same as "identity changed", which is the
    /// correct, safe behavior: it forces a fresh open rather than risking a
    /// stale-offset read against the wrong file.
    identity: Option<(u64, u64)>,
    /// Whether the current gzip stream was deliberately left unread (e.g. `read_from: end`),
    /// as opposed to `file_position == 0` meaning "not decoded yet, about to start from zero."
    /// Lets `reactivate` tell the two apart instead of wrongly replaying a skipped backlog.
    gzip_read_skipped: bool,
    /// Whether the current path contains gzip data. Gzip's logical position is decompressed bytes,
    /// so it cannot be compared with the raw on-disk size when establishing an idle baseline.
    is_gzip: bool,
    /// Raw size/mtime captured when the current gzip reader was opened. Comparing this with the
    /// final active-state metadata lets `deactivate` detect writes during the transition without
    /// forcing a full re-decode after every quiet idle period.
    gzip_raw_metadata: Option<(u64, Option<SystemTime>)>,
    is_dead: bool,
    last_seen: Instant,
    max_line_bytes: usize,
    line_delimiter: Bytes,
}

/// The device/inode pair used to identify a file across a rename.
pub type FileIdentity = (u64, u64);

impl FileWatcher {
    /// Create a new `FileWatcher`
    ///
    /// The input path will be used by `FileWatcher` to prime its state
    /// machine. A `FileWatcher` tracks _only one_ file. This function returns
    /// None if the path does not exist or is not readable by the current process.
    ///
    /// If the file is old enough to be excluded by `ignore_before` and its size
    /// on disk already matches the position we'd resume reading from (i.e.
    /// there's no new data waiting), and `idle_on_startup` is `true`, the file
    /// is only opened briefly for a gzip/identity probe: the watcher starts in
    /// the `Idle` state and holds no file handle. This is the core of the fix for
    /// https://github.com/vectordotdev/vector/issues/3567, where a large
    /// number of `ignore_older`-excluded files would otherwise each hold open
    /// an unused file handle for as long as they existed on disk.
    ///
    /// `idle_on_startup` should be `false` whenever `FileServer::idle_timeout`
    /// is `None` (the user has explicitly opted out of idle-handle-closing
    /// entirely): without gating this fast path on it too, an
    /// `ignore_older`-excluded file would still start `Idle` at discovery
    /// time regardless of `idle_timeout`, since this startup path is a
    /// separate mechanism from the runtime `deactivate()` transition that
    /// `idle_timeout` alone controls -- silently defeating the documented
    /// promise that `idle_timeout: null` restores the prior always-open
    /// behavior.
    pub async fn new(
        path: PathBuf,
        read_from: ReadFrom,
        ignore_before: Option<DateTime<Utc>>,
        max_line_bytes: usize,
        line_delimiter: Bytes,
        idle_on_startup: bool,
    ) -> Result<FileWatcher, std::io::Error> {
        // Cheap stat-first pass. The old, fully-read path still needs one short-lived open to
        // probe gzip and capture identity, but never keeps a handle in the returned watcher.
        let stat = tokio::fs::metadata(&path).await?;
        let modified_time = stat.modified().ok();
        let mut too_old = is_too_old(ignore_before, modified_time);

        if too_old && idle_on_startup {
            // For a *non-gzip* file that's too old, the read position ends up
            // being the same regardless of `read_from`: `(false, true, _)`
            // below always seeks straight to EOF unconditionally, ignoring
            // `Beginning`/`End`, and even a `Checkpoint` position that
            // doesn't match the current size. So the only thing we need
            // before we can decide to stay closed is confirming the file
            // isn't gzip: a gzip file's "too old" handling starts back at
            // position 0 rather than EOF (`(true, true, _)` below), so those
            // still need the full open+decode path to get that right.
            let gzip_check = peek_is_gzipped(&path).await;
            if let Some((is_gzip, identity, probe_size, probe_mtime)) = gzip_check {
                // Use the metadata from the same descriptor as the gzip/identity probe. The
                // initial path stat can be stale if rotation happens before the probe opens it.
                too_old = is_too_old(ignore_before, probe_mtime);
                if !is_gzip && too_old {
                    let idle_since = instant_from_system_time(probe_mtime);
                    let canonical_path = tokio::fs::canonicalize(&path).await.ok();
                    debug!(
                        message = "Starting file watcher in idle state; no unread data and file is older than `ignore_older`.",
                        ?path,
                        file_position = %probe_size,
                    );
                    return Ok(FileWatcher {
                        path,
                        canonical_path,
                        findable: true,
                        path_outside_glob: false,
                        state: WatcherState::Idle {
                            last_known_size: Some(probe_size),
                            last_known_mtime: probe_mtime,
                            idle_since,
                            truncated_while_idle: false,
                            force_recheck: false,
                            pending_partial_line: None,
                            reached_eof: true,
                        },
                        file_position: probe_size,
                        // The gzip probe already gave us the identity, but the handle was closed
                        // before returning, so this watcher still holds no file descriptor.
                        identity: Some(identity),
                        // Confirmed non-gzip by `gzip_check` above.
                        gzip_read_skipped: false,
                        is_gzip: false,
                        gzip_raw_metadata: None,
                        is_dead: false,
                        last_seen: Instant::now(),
                        max_line_bytes,
                        line_delimiter,
                    });
                }
            }
            // Either it's gzip (needs the full open+decode path below to get
            // position 0 vs EOF right) or the file vanished/became
            // unreadable between the stat above and `peek_is_gzipped`'s open
            // (`gzip_check` is `None`) -- either way, fall through to the
            // normal open path, which handles both correctly (and will
            // surface a real error for the latter case).
        }

        let f = open_regular_file(&path).await?;
        let file_info = f.file_info().await?;
        let (devno, ino) = (file_info.portable_dev(), file_info.portable_ino());

        #[cfg(unix)]
        let metadata = file_info;
        #[cfg(windows)]
        let metadata = f.metadata().await?;

        // The path may have changed again after the short-lived startup probe. The descriptor
        // metadata is the authoritative snapshot for the file we are about to read.
        let too_old = is_too_old(ignore_before, metadata.modified().ok());

        let mut reader = BufReader::new(f);

        let gzipped = is_gzipped(&mut reader).await?;
        let gzip_raw_metadata = gzipped.then(|| (metadata.len(), metadata.modified().ok()));

        // Determine the actual position at which we should start reading
        let (reader, file_position, gzip_read_skipped): (
            Box<dyn AsyncBufRead + Send + Unpin>,
            FilePosition,
            bool,
        ) = match (gzipped, too_old, read_from) {
            (true, true, _) => {
                debug!(
                    message = "Not reading gzipped file older than `ignore_older`.",
                    ?path,
                );
                (Box::new(null_reader()), 0, true)
            }
            (true, _, ReadFrom::Checkpoint(file_position)) => {
                debug!(
                    message = "Not re-reading gzipped file with existing stored offset.",
                    ?path,
                    %file_position
                );
                (Box::new(null_reader()), file_position, true)
            }
            // TODO: This may become the default, leading us to stop reading gzipped files that
            // we were reading before. Should we merge this and the next branch to read
            // compressed file from the beginning even when `read_from = "end"` (implicitly via
            // default or explicitly via config)?
            (true, _, ReadFrom::End) => {
                debug!(
                    message = "Can't read from the end of already-compressed file.",
                    ?path,
                );
                (Box::new(null_reader()), 0, true)
            }
            (true, false, ReadFrom::Beginning) => (
                Box::new(BufReader::new(gzip_multiple_decoder(reader))),
                0,
                false,
            ),
            (false, true, _) => {
                let pos = reader.seek(SeekFrom::End(0)).await.unwrap();
                (Box::new(reader), pos, false)
            }
            (false, false, ReadFrom::Checkpoint(file_position)) => {
                let pos = reader.seek(SeekFrom::Start(file_position)).await.unwrap();
                (Box::new(reader), pos, false)
            }
            (false, false, ReadFrom::Beginning) => {
                let pos = reader.seek(SeekFrom::Start(0)).await.unwrap();
                (Box::new(reader), pos, false)
            }
            (false, false, ReadFrom::End) => {
                let pos = reader.seek(SeekFrom::End(0)).await.unwrap();
                (Box::new(reader), pos, false)
            }
        };

        let ts = instant_from_system_time(metadata.modified().ok());
        let canonical_path = tokio::fs::canonicalize(&path).await.ok();

        Ok(FileWatcher {
            path,
            canonical_path,
            findable: true,
            path_outside_glob: false,
            state: WatcherState::Active {
                reader,
                reached_eof: false,
                last_read_attempt: ts,
                last_read_success: ts,
                read_retry_delay: EOF_READ_BACKOFF_MIN,
                buf: BytesMut::new(),
            },
            file_position,
            identity: Some((devno, ino)),
            gzip_read_skipped,
            is_gzip: gzipped,
            gzip_raw_metadata,
            is_dead: false,
            last_seen: ts,
            max_line_bytes,
            line_delimiter,
        })
    }

    /// Update the path this watcher tracks after `FileServer`'s glob-rescan
    /// detects that the same fingerprint now resolves to a different path
    /// (i.e. the file was renamed/rotated).
    ///
    /// This briefly opens the file to re-verify identity (dev/inode) and, if
    /// the identity changed, to determine the correct read position/gzip
    /// state for the new path -- there is no portable way to compare file
    /// identity without a handle (`GetFileInformationByHandle` is required on
    /// Windows even for files we've never read). If the watcher was `Idle`
    /// before this call and remains eligible to be idle afterwards (the
    /// resolved dev/inode is unchanged, i.e. this was a pure rename with no
    /// new data), the handle opened here is not retained: we transition back
    /// to `Idle` immediately rather than leaving it `Active`. This keeps a
    /// pure rename of an idle file from permanently pinning a handle open,
    /// while still guaranteeing we never resume reading a *different* file's
    /// content from a stale offset (the concern `update_path` exists to
    /// address in the first place).
    pub async fn update_path(&mut self, path: PathBuf) -> io::Result<()> {
        let was_idle = self.is_idle();

        let file_handle = open_regular_file(&path).await?;

        let file_info = file_handle.file_info().await?;
        let new_identity = (file_info.portable_dev(), file_info.portable_ino());
        let raw_metadata = file_handle.metadata().await.ok();
        let canonical_path = tokio::fs::canonicalize(&path).await.ok();
        if Some(new_identity) != self.identity {
            // Keep identity and reader tied to the same descriptor. Opening the path a second
            // time would allow a rotation between the two opens to pair the first file's
            // identity with the replacement file's contents.
            let mut reader = BufReader::new(file_handle);
            let gzipped = is_gzipped(&mut reader).await?;
            let gzip_raw_metadata = gzipped.then(|| {
                raw_metadata
                    .as_ref()
                    .map(|metadata| (metadata.len(), metadata.modified().ok()))
            });
            let (new_reader, new_gzip_read_skipped): (Box<dyn AsyncBufRead + Send + Unpin>, bool) =
                if gzipped {
                    (
                        Box::new(BufReader::new(gzip_multiple_decoder(reader))),
                        false,
                    )
                } else {
                    reader.seek(io::SeekFrom::Start(0)).await?;
                    (Box::new(reader), false)
                };

            self.identity = Some(new_identity);
            self.gzip_read_skipped = new_gzip_read_skipped;
            self.is_gzip = gzipped;
            self.gzip_raw_metadata = gzip_raw_metadata.flatten();
            self.file_position = 0;
            self.state = WatcherState::Active {
                reader: new_reader,
                reached_eof: false,
                last_read_attempt: Instant::now(),
                last_read_success: Instant::now(),
                read_retry_delay: EOF_READ_BACKOFF_MIN,
                buf: BytesMut::new(),
            };
        } else if was_idle {
            // Same file (dev/inode unchanged), just renamed, and it was
            // `Idle` before we got here: don't let re-verifying identity
            // above leave us stuck `Active`. Drop the handle we just opened
            // and go straight back to `Idle` under the new path. `deactivate`
            // stats `self.path`, so update it first.
            drop(file_handle);
            self.path = path;
            self.canonical_path = canonical_path;
            if self.is_gzip {
                self.gzip_raw_metadata =
                    raw_metadata.map(|metadata| (metadata.len(), metadata.modified().ok()));
            }
            self.deactivate().await;
            return Ok(());
        } else if let WatcherState::Active {
            reached_eof,
            read_retry_delay,
            ..
        } = &mut self.state
        {
            *reached_eof = false;
            *read_retry_delay = EOF_READ_BACKOFF_MIN;
        }
        if self.is_gzip {
            self.gzip_raw_metadata =
                raw_metadata.map(|metadata| (metadata.len(), metadata.modified().ok()));
        }
        self.path = path;
        self.canonical_path = canonical_path;
        Ok(())
    }

    /// Whether this watcher currently holds an open file handle.
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(self.state, WatcherState::Active { .. })
    }

    #[inline]
    pub fn is_idle(&self) -> bool {
        matches!(self.state, WatcherState::Idle { .. })
    }

    pub fn set_file_findable(&mut self, f: bool) {
        self.findable = f;
        if f {
            self.last_seen = Instant::now();
            self.path_outside_glob = false;
        }
    }

    /// Mark this watcher as still live at a path outside the configured include patterns. This
    /// is used after an idle watcher is found by identity following a rotation, so subsequent
    /// glob passes do not make `poll_idle_watchers` abandon the rotated inode.
    pub fn mark_path_outside_glob(&mut self) {
        self.path_outside_glob = true;
        self.last_seen = Instant::now();
    }

    #[inline]
    pub fn path_is_outside_glob(&self) -> bool {
        self.path_outside_glob
    }

    /// Check whether the current path still resolves to the tracked file identity. This is a
    /// single-file check used to avoid rescanning an entire archive tree on every polling pass
    /// for an outside-glob idle watcher; a tree scan is only needed once this path disappears or
    /// resolves to a replacement.
    pub fn path_has_tracked_identity(
        &self,
    ) -> impl std::future::Future<Output = bool> + Send + 'static {
        let path = self.path.clone();
        let identity = self.identity;
        async move {
            let Some(identity) = identity else {
                return false;
            };
            path_identity(&path).await == Some(identity)
        }
    }

    /// Find this watcher's previously opened inode at a path named by a notify event or below its
    /// current parent directory. The event candidates cover destinations outside the include glob;
    /// the recursive parent scan also handles archive subdirectories in polling mode.
    pub async fn find_renamed_path(
        &self,
        event_paths: Option<&HashSet<PathBuf>>,
    ) -> Option<PathBuf> {
        let event_identities = match event_paths {
            Some(paths) => Some(identify_event_paths(paths).await),
            None => None,
        };
        if let Some(path) = self.find_renamed_path_in_identities(event_identities.as_ref()) {
            return Some(path);
        }

        let root = self.rename_search_root()?;
        let tree_identities = identify_paths_in_tree(&root).await;
        self.find_renamed_path_with_identities(None, Some(&tree_identities))
            .await
    }

    /// Return the absolute directory below which polling-based rename recovery searches.
    pub fn rename_search_root(&self) -> Option<PathBuf> {
        let cwd = std::env::current_dir().ok();
        self.path.parent().map(|parent| {
            let parent = if parent.as_os_str().is_empty() {
                Path::new(".")
            } else {
                parent
            };
            crate::absolutize(parent, cwd.as_deref())
        })
    }

    /// Find this watcher's inode in identities already collected from notify event paths.
    /// Keeping this step synchronous lets callers try the cheap, precise event candidates before
    /// starting the potentially expensive recursive parent-directory scan.
    pub(crate) fn find_renamed_path_in_identities(
        &self,
        identities: Option<&HashMap<FileIdentity, Vec<PathBuf>>>,
    ) -> Option<PathBuf> {
        let identity = self.identity?;
        let cwd = std::env::current_dir().ok();
        let current_path = crate::absolutize(&self.path, cwd.as_deref());
        identities?.get(&identity)?.iter().find_map(|candidate| {
            (crate::absolutize(candidate, cwd.as_deref()) != current_path)
                .then(|| candidate.clone())
        })
    }

    /// Find this watcher's inode using event-path identities prepared once for the whole polling
    /// pass. Unlike `find_renamed_path`, this avoids reopening every event candidate and scanning
    /// the same parent directory for every idle watcher when a rename burst affects many files.
    pub fn find_renamed_path_with_identities(
        &self,
        event_identities: Option<&HashMap<FileIdentity, Vec<PathBuf>>>,
        tree_identities: Option<&HashMap<FileIdentity, Vec<PathBuf>>>,
    ) -> impl std::future::Future<Output = Option<PathBuf>> + Send + 'static {
        let event_candidate = self.find_renamed_path_in_identities(event_identities);
        let tree_candidate = self.find_renamed_path_in_identities(tree_identities);
        let identity = self.identity;
        let cwd = std::env::current_dir().ok();
        let current_path = crate::absolutize(&self.path, cwd.as_deref());
        let parent = self.rename_search_root();
        let tree_was_indexed = tree_identities.is_some();

        async move {
            if event_candidate.is_some() {
                return event_candidate;
            }
            if tree_candidate.is_some() {
                return tree_candidate;
            }

            if tree_was_indexed {
                None
            } else {
                let parent = parent?;
                let tree_identities = identify_paths_in_tree(&parent).await;
                // No event candidate was found above, so the tree scan is the only remaining
                // source of a possible replacement path.
                tree_identities.get(&identity?).and_then(|candidates| {
                    candidates.iter().find_map(|candidate| {
                        (crate::absolutize(candidate, cwd.as_deref()) != current_path)
                            .then(|| candidate.clone())
                    })
                })
            }
        }
    }

    pub fn file_findable(&self) -> bool {
        self.findable
    }

    pub fn set_dead(&mut self) {
        self.is_dead = true;
    }

    pub fn dead(&self) -> bool {
        self.is_dead
    }

    pub fn get_file_position(&self) -> FilePosition {
        self.file_position
    }

    pub fn canonical_path(&self) -> Option<&Path> {
        self.canonical_path.as_deref()
    }

    /// Cheaply (via `fs::metadata`, no `File::open`) check whether an `Idle`
    /// watcher's file has changed since we last looked (grown, shrunk, or had
    /// its mtime bumped). Returns `Ok(true)` if the watcher should be promoted
    /// back to `Active` (i.e. reopened) by the caller. No-ops (returns
    /// `Ok(false)`) for `Active` watchers.
    ///
    /// This does not perform the reopen itself: `FileServer` calls
    /// `reactivate` to do that once it decides to, since the reopen also
    /// needs to handle the "the file was replaced by a same-named different
    /// file" case, which is otherwise already handled by the fingerprint-based
    /// rename detection in `FileServer`.
    pub async fn check_for_new_data(&mut self) -> io::Result<bool> {
        let WatcherState::Idle {
            last_known_size,
            last_known_mtime,
            idle_since,
            truncated_while_idle,
            force_recheck,
            pending_partial_line,
            ..
        } = &mut self.state
        else {
            return Ok(false);
        };

        let stat = tokio::fs::metadata(&self.path).await?;
        let new_size = stat.len();
        let new_mtime = stat.modified().ok();

        // The real baseline (`last_known_size`/`last_known_mtime`) is never destroyed to force a
        // retry -- see `invalidate_idle_bookkeeping`'s doc comment for why an earlier version of
        // this that clobbered it with a sentinel was wrong. A missing size means the stat during
        // deactivation failed, so the first successful poll is a change but cannot prove a shrink.
        let sizes_or_mtimes_differ =
            last_known_size.is_none_or(|size| new_size != size) || new_mtime != *last_known_mtime;
        let changed = sizes_or_mtimes_differ || *force_recheck;
        *force_recheck = false;

        // Latch, don't overwrite: a truncate seen on *this* poll must still be remembered even if
        // a later poll (or `reactivate`'s own final check) finds the file has since grown back
        // past `file_position` again, since that "grown past the old position" state is exactly
        // what an ordinary, never-truncated append would also look like. See the field doc on
        // `WatcherState::Idle::truncated_while_idle` for why a point-in-time comparison alone,
        // taken only at reactivation time, isn't sufficient. This check is against the real
        // baseline (never a sentinel), so it correctly fires for a genuine truncation regardless
        // of whether `force_recheck` also happens to be set on this same poll.
        if last_known_size.is_some_and(|size| new_size < size) {
            *truncated_while_idle = true;
            // The pre-truncation offset/bytes no longer correspond to anything on disk.
            *pending_partial_line = None;
        }

        // Always keep our idle bookkeeping current so that a subsequent
        // truncation-then-refill (or vice versa) is still detected relative
        // to what we most recently observed.
        *last_known_size = Some(new_size);
        *last_known_mtime = new_mtime;
        if changed {
            // Reset the idle clock: something happened, so this file is not
            // eligible for idle-driven removal right now even though it's
            // about to be promoted back to `Active` by the caller anyway.
            *idle_since = Instant::now();
        }

        Ok(changed)
    }

    /// Force the next `check_for_new_data` call to report a change, regardless of what it
    /// actually observes.
    ///
    /// Call this after a failed `reactivate()` that followed a `check_for_new_data` reporting
    /// `true`. `check_for_new_data` unconditionally records whatever size/mtime it just observed
    /// (so that a subsequent truncate-then-refill is still detected relative to the most recent
    /// state, not stale pre-truncation values) *before* the caller has had a chance to act on the
    /// "changed" result. If the caller's `reactivate()` then fails (e.g. a transient permission
    /// or I/O error) and the file doesn't change again in the meantime, the next poll would
    /// compare against the size/mtime already recorded from the failed attempt, see no
    /// difference, and never retry -- silently stranding the watcher `Idle` with unread data
    /// sitting on disk. Setting `force_recheck` guarantees the next poll reports a change and
    /// retries, no matter what it actually observes.
    ///
    /// Deliberately does *not* touch `last_known_size`/`last_known_mtime` (an earlier version of
    /// this clobbered `last_known_size` with an impossible `u64::MAX` sentinel instead of using a
    /// separate flag). Destroying the real baseline that way meant a genuine truncation occurring
    /// *after* this was called but *before* the next poll would be completely undetectable on
    /// that poll: `check_for_new_data`'s shrink comparison has nothing real left to compare the
    /// new, smaller size against, since the "last known size" it would be comparing against is
    /// itself a fabricated value, not the file's actual prior size. Keeping the real baseline
    /// intact and layering `force_recheck` on top instead lets `check_for_new_data` still
    /// correctly detect a real shrink on the very poll that also honors this forced retry.
    ///
    /// No-op if the watcher isn't `Idle` (e.g. it was already promoted back to `Active` by the
    /// time this is called).
    pub fn invalidate_idle_bookkeeping(&mut self) {
        if let WatcherState::Idle { force_recheck, .. } = &mut self.state {
            *force_recheck = true;
        }
    }

    /// Promote an `Idle` watcher back to `Active`: (re)open the file and seek
    /// to `file_position`. Also handles (re-)detecting gzip compression,
    /// since that detection was deferred when we skipped the initial open.
    ///
    /// If the reopened file's identity (dev/inode) doesn't match what this watcher *previously
    /// confirmed by having actually opened the file* (i.e. `self.identity` was `Some`, not
    /// `None`), the file at this path has been replaced since we went idle: the same-path
    /// rotation case (`discover`'s fingerprint-based rename detection only catches renames, i.e.
    /// a path change; a rewrite-in-place under an unchanged path -- possible if the new content's
    /// fingerprint happens to collide with the old one, since the default strategy only hashes
    /// the first line -- looks identical to "nothing happened" from `discover`'s point of view).
    /// In that case we must not seek to the stale `file_position`: it's a byte offset into a file
    /// that no longer exists, so seeking to it on the new file would silently skip (if the new
    /// file is longer) or read nothing until it grows past that point (if shorter) -- either way
    /// losing the new file's opening bytes. Start over from position 0 instead.
    ///
    /// A `self.identity` of `None`, by contrast, means identity is still "unconfirmed" (for
    /// example, if the short-lived startup probe could not complete). That's not evidence of a
    /// replacement, so unlike a real identity mismatch, it must not reset `file_position`: doing
    /// so would re-read a file's entire old content (which `ignore_older` deliberately skipped)
    /// the first time it receives new data, since `file_position` holds the file's size as of
    /// discovery rather than a checkpoint from a previous read.
    ///
    /// **Known limitation**: when identity is unavailable, this function cannot distinguish "an
    /// `ignore_older`-excluded file received its first append" from "that file was replaced (not
    /// renamed) by a different, larger file at the same path, whose content happens to fingerprint
    /// identically to the old one under the default first-line-only strategy" before its first
    /// reactivation. The former (by far the common case) requires resuming from the retained
    /// `file_position`; the latter would need resuming from 0. Since a replacement can't be told
    /// apart from growth here, this function assumes growth.
    ///
    /// Separately, even when the identity is unchanged (the same inode is still at this path --
    /// no rename/replace happened), the file can still have been truncated in place while idle
    /// (e.g. `logrotate`'s `copytruncate`, or an application that truncates and rewrites its own
    /// log). A truncation must reset `file_position` to 0 just as a real identity change does --
    /// seeking to a stale `file_position` on a file that's been truncated (whether or not it's
    /// since grown back past that same offset with unrelated new content) means either seeking
    /// past EOF (silently losing everything written until the file grows past the old position
    /// again) or, worse, silently reading unrelated new bytes as if they were a continuation of
    /// the old content. This is why `truncated_while_idle` is a latch set by `check_for_new_data`
    /// across the *whole* idle period rather than something `reactivate` could reliably re-derive
    /// from a single point-in-time size comparison of its own: a truncate observed by one poll,
    /// followed by a refill past the old `file_position` observed by a later poll, would otherwise
    /// look identical to ordinary growth by the time `reactivate` gets a chance to look.
    ///
    /// **Known limitation**: this still can't help if the truncate *and* the regrowth both happen
    /// between two polls, with neither `check_for_new_data` call ever independently observing the
    /// intermediate (truncated) state -- there is, at that point, no state left on disk to detect
    /// it from after the fact. This is a fundamental limit of polling for changes, not something
    /// specific to this idle-handle-closing mechanism: `file_discovery_mode: polling`'s pre-existing
    /// handling of *active* (never-idle) files has the same blind spot for a within-one-interval
    /// truncate-then-refill, and no polling-based approach (as opposed to synchronous OS-level
    /// notification of every write, which isn't what `fs::metadata`-based polling provides even
    /// under `file_discovery_mode: notify`, since that only wakes up the same poll sooner, it
    /// doesn't add fidelity to what a single poll can observe) can close this gap.
    ///
    /// No-op if the watcher is already `Active`.
    pub async fn reactivate(&mut self) -> io::Result<()> {
        if self.is_active() {
            return Ok(());
        }

        let truncated_while_idle = matches!(
            self.state,
            WatcherState::Idle {
                truncated_while_idle: true,
                ..
            }
        );

        let f = open_regular_file(&self.path).await?;
        let file_info = f.file_info().await?;
        let new_identity = (file_info.portable_dev(), file_info.portable_ino());
        let raw_metadata = f.metadata().await?;
        let identity_changed = matches!(self.identity, Some(old) if old != new_identity);
        let old_file_position = self.file_position;
        let mut file_position = old_file_position;
        let mut gzip_read_skipped = self.gzip_read_skipped;

        let mut reader = BufReader::new(f);
        let gzipped = is_gzipped(&mut reader).await?;
        let canonical_path = tokio::fs::canonicalize(&self.path).await.ok();
        let gzip_raw_metadata = gzipped.then(|| (raw_metadata.len(), raw_metadata.modified().ok()));
        let format_changed = gzipped != self.is_gzip;

        // Final direct check in case `check_for_new_data` was never called before this. Skipped
        // for gzip: `file_position` is a decompressed offset, not comparable to on-disk size.
        let truncated_at_reactivation = !gzipped && raw_metadata.len() < old_file_position;
        if identity_changed || format_changed {
            debug!(
                message = "Idle watcher's file identity or compression format changed on \
                           reactivation; resuming from the start rather than the stale \
                           checkpoint offset.",
                path = ?self.path,
            );
            file_position = 0;
            gzip_read_skipped = false;
        } else if truncated_while_idle || truncated_at_reactivation {
            debug!(
                message = "Idle watcher's file was truncated in place while idle (same \
                           identity). Resuming from the start rather than seeking past \
                           stale, since-invalidated content.",
                path = ?self.path,
            );
            file_position = 0;
            gzip_read_skipped = false;
        }

        let (reader, file_position, gzip_read_skipped): (
            Box<dyn AsyncBufRead + Send + Unpin>,
            FilePosition,
            bool,
        ) = if gzipped {
            if gzip_read_skipped {
                // Deliberately unread (e.g. `read_from: end`): no decoded prefix to resume from.
                (Box::new(null_reader()), file_position, true)
            } else if file_position == 0 {
                (
                    Box::new(BufReader::new(gzip_multiple_decoder(reader))),
                    0,
                    false,
                )
            } else {
                // `GzipDecoder` can't seek to a decompressed offset, so resume by redecoding from
                // the start and discarding the already-emitted prefix via `SkipPrefixReader`.
                // This also naturally picks up any member appended after `file_position`.
                let skip = file_position;
                (
                    Box::new(BufReader::new(SkipPrefixReader::new(
                        gzip_multiple_decoder(reader),
                        skip,
                    ))),
                    skip,
                    false,
                )
            }
        } else {
            // Propagate seek errors instead of guessing the position; the caller retries from
            // `Idle` on `Err`, which is safer than reading from an unknown offset.
            let pos = reader.seek(SeekFrom::Start(file_position)).await?;
            (Box::new(reader), pos, false)
        };
        self.identity = Some(new_identity);
        self.canonical_path = canonical_path;
        self.gzip_read_skipped = gzip_read_skipped;
        self.is_gzip = gzipped;
        self.gzip_raw_metadata = gzip_raw_metadata;

        self.file_position = file_position;
        self.state = WatcherState::Active {
            reader,
            reached_eof: false,
            last_read_attempt: Instant::now(),
            last_read_success: Instant::now(),
            read_retry_delay: EOF_READ_BACKOFF_MIN,
            buf: BytesMut::new(),
        };

        debug!(
            message = "File watcher reactivated from idle state.",
            path = ?self.path,
            file_position = %self.file_position,
        );

        Ok(())
    }

    /// Transition an `Active` watcher to `Idle`, closing its file handle.
    /// No-op if already `Idle`.
    pub async fn deactivate(&mut self) {
        let WatcherState::Active {
            buf,
            last_read_success,
            reached_eof,
            ..
        } = &self.state
        else {
            return;
        };
        let reached_eof = *reached_eof;
        // Preserve the time of the last successful read (i.e. last-observed activity), not
        // "now" (the moment of deactivation): `FileServer` only calls `deactivate` once a watcher
        // has already been sitting EOF'd and quiet for `idle_timeout`, so by the time we get here
        // `last_read_success` is already well in the past. Stamping `idle_since` with `Instant::now()`
        // instead would silently add another `idle_timeout`'s worth of delay on top of the
        // documented `remove_after`-since-EOF grace period every time `remove_after_secs` exceeds
        // `idle_timeout_secs`.
        let idle_since = *last_read_success;
        let old_file_position = self.file_position;

        // `buf` holds bytes already consumed from the reader (and counted
        // into `file_position`) for a line that hasn't seen its delimiter
        // yet -- `read_until_with_max_size` advances `position` for every
        // byte it reads into `buf`, delimiter or not, on the assumption that
        // the very next call will pick up exactly where it left off and
        // eventually complete the line. Idle-izing throws `buf` away (it's
        // part of the `Active` state we're about to replace), so unless we
        // rewind `file_position` back behind those bytes here, `reactivate`
        // would resume reading *after* them: the partial line would never be
        // completed, and its bytes -- still sitting on disk -- would simply
        // never be read. Rewinding means we'll read them again from disk
        // once new data (including, at minimum, this file's own trailing
        // delimiter) shows up, same as if we'd never buffered them at all.
        // Saturating, not a bare subtraction: if the file was truncated out
        // from under an `Active` read (a pre-existing sharp edge of file
        // watching in general, not something this rewind introduces), the
        // buffered byte count could in principle exceed `file_position`. In
        // that case there's nothing meaningful to rewind to; clamping to 0
        // is at least as safe as what an in-progress read would already be
        // dealing with (`read_until_with_max_size` doesn't special-case
        // mid-read truncation either).
        //
        // Also clone the buffered bytes themselves (not just their count) before rewinding:
        // `pending_partial_line` retains them, paired with the offset they started at (i.e.
        // `file_position` *before* the rewind below), purely so `FileServer` can salvage them as
        // a final record if this watcher is later reaped while still `Idle` -- see that field's
        // doc comment for why an `Idle` watcher has no other way to flush them, unlike an
        // `Active` one. A no-op clone (empty `Bytes`) when there's nothing buffered.
        let unterminated_bytes = buf.len() as u64;
        let rewound_file_position = self.file_position.saturating_sub(unterminated_bytes);
        let pending_partial_line = if buf.is_empty() {
            None
        } else {
            Some((rewound_file_position, buf.clone().freeze()))
        };
        let metadata = tokio::fs::metadata(&self.path).await.ok();
        if !self.is_gzip
            && metadata
                .as_ref()
                .is_some_and(|stat| stat.len() > old_file_position)
        {
            // A write can race with the EOF read and land before this transition. Do not close
            // the handle in that case: treating the current size as the idle baseline would make
            // the new bytes invisible to the next poll. Keep the active reader so the normal read
            // loop can observe them.
            debug!(
                message = "Keeping file watcher active because data arrived during idle transition.",
                path = ?self.path,
                file_position = %self.file_position,
            );
            return;
        }

        self.file_position = rewound_file_position;

        // For a plain file the stat is a trustworthy raw-size baseline once it has not raced past
        // the pre-rewind position. Gzip's decoded position cannot be compared with raw size, so
        // force a re-check only if its raw metadata changed while the reader was active.
        let last_known_size = metadata.as_ref().map(|stat| stat.len());
        let force_recheck = self.is_gzip
            && !self.gzip_read_skipped
            && match (self.gzip_raw_metadata, metadata.as_ref()) {
                (Some((size, mtime)), Some(stat)) => {
                    size != stat.len() || mtime != stat.modified().ok()
                }
                _ => true,
            };
        let last_known_mtime = metadata.and_then(|stat| stat.modified().ok());

        debug!(
            message = "File watcher deactivated to idle state; file handle closed.",
            path = ?self.path,
            file_position = %self.file_position,
            rewound_unterminated_bytes = %unterminated_bytes,
        );

        self.state = WatcherState::Idle {
            last_known_size,
            last_known_mtime,
            idle_since,
            // A fresh Idle period starts here: `file_position` above already reflects the
            // buffered-but-unterminated-line rewind (if any), which is a correction to where we
            // resume reading, not evidence the file itself was truncated on disk. There's nothing
            // yet for a subsequent `check_for_new_data` poll to have observed shrinking.
            truncated_while_idle: false,
            force_recheck,
            pending_partial_line,
            reached_eof,
        };
    }

    /// Read a single line from the underlying file
    ///
    /// This function will attempt to read a new line from its file, blocking,
    /// up to some maximum but unspecified amount of time. `read_line` will open
    /// a new file handler as needed, transparently to the caller.
    pub(super) async fn read_line(&mut self) -> io::Result<RawLineResult> {
        self.track_read_attempt();

        let WatcherState::Active { reader, buf, .. } = &mut self.state else {
            // Should not be called while idle; `FileServer` gates calls to
            // `read_line` on `should_read`, which is false for idle watchers.
            return Ok(RawLineResult {
                raw_line: None,
                discarded_for_size_and_truncated: Vec::new(),
            });
        };

        let initial_position = self.file_position;
        let read_result = read_until_with_max_size(
            reader.as_mut(),
            &mut self.file_position,
            self.line_delimiter.as_ref(),
            buf,
            self.max_line_bytes,
        )
        .await;
        // The borrow of `self.state` (via `reader`/`buf` above) ends here,
        // once `read_until_with_max_size` returns; everything below is free
        // to borrow `self` again, including re-matching on `self.state` to
        // get at `buf`/`reached_eof`, which is guaranteed to still be
        // `Active` since nothing else runs concurrently on this watcher.
        match read_result {
            Ok(ReadResult {
                successfully_read: Some(_),
                discarded_for_size_and_truncated,
            }) => {
                let WatcherState::Active { buf, .. } = &mut self.state else {
                    unreachable!("state is Active: nothing transitions it mid-read")
                };
                let bytes = buf.split().freeze();
                self.track_read_success();
                Ok(RawLineResult {
                    raw_line: Some(RawLine {
                        offset: initial_position,
                        bytes,
                    }),
                    discarded_for_size_and_truncated,
                })
            }
            Ok(ReadResult {
                successfully_read: None,
                discarded_for_size_and_truncated,
            }) => {
                if !self.file_findable() && !self.path_outside_glob {
                    self.set_dead();
                    // File has been deleted, so return what we have in the buffer, even though it
                    // didn't end with a newline. This is not a perfect signal for when we should
                    // give up waiting for a newline, but it's decent.
                    let WatcherState::Active {
                        buf, reached_eof, ..
                    } = &mut self.state
                    else {
                        unreachable!("state is Active: nothing transitions it mid-read")
                    };
                    let buf = buf.split().freeze();
                    if buf.is_empty() {
                        // EOF
                        *reached_eof = true;
                        Ok(RawLineResult {
                            raw_line: None,
                            discarded_for_size_and_truncated,
                        })
                    } else {
                        Ok(RawLineResult {
                            raw_line: Some(RawLine {
                                offset: initial_position,
                                bytes: buf,
                            }),
                            discarded_for_size_and_truncated,
                        })
                    }
                } else {
                    self.track_read_eof();
                    Ok(RawLineResult {
                        raw_line: None,
                        discarded_for_size_and_truncated,
                    })
                }
            }
            Err(e) => {
                if let io::ErrorKind::NotFound = e.kind() {
                    self.set_dead();
                }
                Err(e)
            }
        }
    }

    #[inline]
    fn track_read_attempt(&mut self) {
        if let WatcherState::Active {
            last_read_attempt, ..
        } = &mut self.state
        {
            *last_read_attempt = Instant::now();
        }
    }

    #[inline]
    fn track_read_success(&mut self) {
        if let WatcherState::Active {
            reached_eof,
            read_retry_delay,
            last_read_success,
            ..
        } = &mut self.state
        {
            *reached_eof = false;
            *read_retry_delay = EOF_READ_BACKOFF_MIN;
            *last_read_success = Instant::now();
        }
    }

    #[inline]
    fn track_read_eof(&mut self) {
        if let WatcherState::Active {
            reached_eof,
            read_retry_delay,
            ..
        } = &mut self.state
        {
            *read_retry_delay = if *reached_eof {
                std::cmp::min(read_retry_delay.saturating_mul(2), EOF_READ_BACKOFF_MAX)
            } else {
                EOF_READ_BACKOFF_MIN
            };
            *reached_eof = true;
        }
    }

    /// Time of the last successful read. For `Idle` watchers (which cannot be
    /// actively reading), this is always "now", so that `remove_after`-style
    /// grace-period logic in `FileServer` does not immediately consider an
    /// idle file eligible for removal purely because it went idle; removal
    /// eligibility for idle files is instead driven by `last_seen`/findability
    /// via the normal glob-rescan path.
    #[inline]
    pub fn last_read_success(&self) -> Instant {
        match &self.state {
            WatcherState::Active {
                last_read_success, ..
            } => *last_read_success,
            WatcherState::Idle { .. } => Instant::now(),
        }
    }

    /// Clear any backoff/throttle state so the very next `should_read` check returns `true`
    /// (unless the watcher is `Idle`, which this is a no-op for). Call this when an external
    /// signal (a notify filesystem event naming this watcher's path) indicates new data may be
    /// available, so `should_read`'s EOF backoff and quiet-file throttle -- both of which exist to
    /// pace *unprompted* polling -- don't delay a read that a concrete signal just justified.
    ///
    /// Without this, a notify event arriving for a watcher that: (a) is mid-EOF-backoff (up to
    /// `EOF_READ_BACKOFF_MAX` = 250ms stale), or (b) has been quiet for over 10 seconds and was
    /// merely polled (not necessarily successfully) within the last 10 seconds -- the "throttle
    /// further attempts to once per 10s" branch of `should_read` -- would still have its read
    /// suppressed until that independent timer elapsed on its own, defeating notify mode's promise
    /// of prompt wakeups for however long is left on it.
    #[inline]
    pub fn mark_ready_to_read(&mut self) {
        if let WatcherState::Active {
            reached_eof,
            last_read_attempt,
            read_retry_delay,
            ..
        } = &mut self.state
        {
            *reached_eof = false;
            *read_retry_delay = EOF_READ_BACKOFF_MIN;
            // Back-date rather than leaving as-is: `should_read`'s quiet-file throttle requires
            // `last_read_attempt.elapsed() > 10s` as one of its two ways to pass, so simply
            // clearing `reached_eof` isn't sufficient on its own to guarantee the very next check
            // passes.
            *last_read_attempt = Instant::now() - Duration::from_secs(11);
        }
    }

    #[inline]
    pub fn should_read(&self) -> bool {
        let WatcherState::Active {
            reached_eof,
            last_read_attempt,
            last_read_success,
            read_retry_delay,
            ..
        } = &self.state
        else {
            // Idle watchers hold no reader; `FileServer` polls them via
            // `check_for_new_data` on the glob-rescan cadence instead.
            return false;
        };

        if *reached_eof && last_read_attempt.elapsed() < *read_retry_delay {
            return false;
        }

        last_read_success.elapsed() < Duration::from_secs(10)
            || last_read_attempt.elapsed() > Duration::from_secs(10)
    }

    #[inline]
    pub fn last_seen(&self) -> Instant {
        self.last_seen
    }

    #[inline]
    pub fn reached_eof(&self) -> bool {
        match &self.state {
            WatcherState::Active { reached_eof, .. } => *reached_eof,
            WatcherState::Idle { reached_eof, .. } => *reached_eof,
        }
    }

    /// How long it has been since this watcher last successfully read data
    /// while `Active`, or since it became `Active` if it has never
    /// successfully read anything yet. Used by `FileServer` to decide when an
    /// `Active`-but-quiet watcher should be moved to `Idle`.
    #[inline]
    pub fn idle_for(&self) -> Option<Duration> {
        match &self.state {
            WatcherState::Active {
                last_read_success, ..
            } => Some(last_read_success.elapsed()),
            WatcherState::Idle { .. } => None,
        }
    }

    /// How long this watcher has been sitting in the `Idle` state without any
    /// detected change (growth, truncation, or mtime bump). `None` if the
    /// watcher is `Active`. Used to drive `remove_after`-style cleanup for
    /// idle files.
    #[inline]
    pub fn idle_since(&self) -> Option<Duration> {
        match &self.state {
            WatcherState::Idle { idle_since, .. } => Some(idle_since.elapsed()),
            WatcherState::Active { .. } => None,
        }
    }

    /// Take the unterminated line this watcher is holding onto, if any (buffered-but-undelimited
    /// bytes, for either `Active` or `Idle`). Call this right before permanently reaping a watcher
    /// via a path that doesn't already go through `read_line` (which has its own flush for the
    /// `Active` case) -- otherwise these bytes are lost for good.
    pub fn take_final_partial_line(&mut self) -> Option<RawLine> {
        match &mut self.state {
            WatcherState::Idle {
                pending_partial_line,
                ..
            } => pending_partial_line
                .take()
                .map(|(offset, bytes)| RawLine { offset, bytes }),
            WatcherState::Active { buf, .. } => {
                if buf.is_empty() {
                    return None;
                }
                let bytes = buf.split().freeze();
                let offset = self.file_position - bytes.len() as u64;
                Some(RawLine { offset, bytes })
            }
        }
    }
}

async fn path_identity(path: &std::path::Path) -> Option<(u64, u64)> {
    let file = open_regular_file(path).await.ok()?;
    let file_info = file.file_info().await.ok()?;
    Some((file_info.portable_dev(), file_info.portable_ino()))
}

/// Open only a regular file without allowing a path/type race to turn the operation into a
/// blocking FIFO read. The descriptor check closes the gap between the initial metadata check and
/// opening the path.
async fn open_regular_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .await?;
    #[cfg(not(unix))]
    let file = File::open(path).await?;

    if !file.metadata().await?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path is not a regular file",
        ));
    }

    Ok(file)
}

/// Resolve the identities of notify rename candidates once per reconciliation pass.
pub async fn identify_event_paths(paths: &HashSet<PathBuf>) -> HashMap<FileIdentity, Vec<PathBuf>> {
    let mut identities = HashMap::new();
    for path in paths {
        if let Some(identity) = path_identity(path).await {
            identities
                .entry(identity)
                .or_insert_with(Vec::new)
                .push(path.clone());
        }
    }
    identities
}

/// Resolve the identities of regular files below `root` once per reconciliation pass.
///
/// Symlinks to regular files are included because the glob provider returns those paths and
/// `File::open` follows them when the watcher identity is captured. Symlinked directories are not
/// traversed, avoiding cycles while still covering the file-rotation case.
pub async fn identify_paths_in_tree(root: &Path) -> HashMap<FileIdentity, Vec<PathBuf>> {
    let mut identities = HashMap::new();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
            continue;
        };
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(_) => break,
            };
            let candidate = entry.path();
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            if file_type.is_dir() {
                directories.push(candidate);
            } else {
                let is_regular_file = if file_type.is_symlink() {
                    tokio::fs::metadata(&candidate)
                        .await
                        .is_ok_and(|metadata| metadata.is_file())
                } else {
                    file_type.is_file()
                };
                if !is_regular_file {
                    continue;
                }
                if let Some(identity) = path_identity(&candidate).await {
                    identities
                        .entry(identity)
                        .or_insert_with(Vec::new)
                        .push(candidate);
                }
            }
        }
    }

    identities
}

async fn is_gzipped(r: &mut BufReader<File>) -> io::Result<bool> {
    let header_bytes = r.fill_buf().await?;
    // WARN: The paired `BufReader::consume` is not called intentionally. If we
    // do we'll chop a decent part of the potential gzip stream off.
    Ok(header_bytes.starts_with(GZIP_MAGIC))
}

fn is_too_old(ignore_before: Option<DateTime<Utc>>, modified_time: Option<SystemTime>) -> bool {
    matches!((ignore_before, modified_time), (Some(ignore_before), Some(modified_time))
        if DateTime::<Utc>::from(modified_time) < ignore_before)
}

/// Cheaply check whether a file starts with the gzip magic bytes, opening and
/// immediately closing it rather than keeping a `FileWatcher`-owned handle
/// around. Used by the `too_old` fast path in `FileWatcher::new` to decide
/// whether a file can safely start `Idle` without going through the full
/// open-and-decode path below: unlike a fully-open `FileWatcher`, this never
/// outlives the single `.await` here, so it doesn't reintroduce the
/// long-lived handle the `Idle` state exists to avoid.
///
/// Returns `None` if the file couldn't be opened or read (e.g. deleted or
/// permissions changed since the earlier `fs::metadata` call); callers should
/// treat that the same as "unknown, fall back to the full open path" rather
/// than assuming either gzip or not.
async fn peek_is_gzipped(
    path: &std::path::Path,
) -> Option<(bool, (u64, u64), u64, Option<SystemTime>)> {
    let f = open_regular_file(path).await.ok()?;
    let file_info = f.file_info().await.ok()?;
    let identity = (file_info.portable_dev(), file_info.portable_ino());
    let metadata = f.metadata().await.ok()?;
    let mut reader = BufReader::new(f);
    Some((
        is_gzipped(&mut reader).await.ok()?,
        identity,
        metadata.len(),
        metadata.modified().ok(),
    ))
}

fn instant_from_system_time(system_time: Option<SystemTime>) -> Instant {
    system_time
        .and_then(|mtime| mtime.elapsed().ok())
        .and_then(|diff| Instant::now().checked_sub(diff))
        .unwrap_or_else(Instant::now)
}

fn null_reader() -> impl AsyncBufRead {
    io::Cursor::new(Vec::new())
}

/// Scratch chunk size for discarding skipped bytes, and also the max bytes discarded per
/// `poll_read` call before yielding back to the executor -- see `SkipPrefixReader::poll_read`.
const SKIP_CHUNK_BYTES: usize = 64 * 1024;

/// Discards the first `skip` bytes read from `inner`, lazily via normal `poll_read` calls.
/// Used to resume a gzip decoder from the start while dropping the already-emitted prefix.
struct SkipPrefixReader<R> {
    inner: R,
    remaining_to_skip: u64,
    scratch: Option<Box<[u8; SKIP_CHUNK_BYTES]>>,
}

impl<R> SkipPrefixReader<R> {
    fn new(inner: R, skip: u64) -> Self {
        Self {
            inner,
            remaining_to_skip: skip,
            scratch: (skip > 0).then(|| Box::new([0u8; SKIP_CHUNK_BYTES])),
        }
    }
}

impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for SkipPrefixReader<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        use std::task::Poll;

        if self.remaining_to_skip == 0 {
            self.scratch = None;
            return std::pin::Pin::new(&mut self.inner).poll_read(cx, buf);
        }

        // A large `remaining_to_skip` (a big already-decoded gzip prefix) could otherwise keep
        // this loop spinning on synchronously-available data for a long time without ever
        // returning to the executor, starving other tasks on the same worker thread. Cap the
        // work done per call and yield (wake + `Pending`) once the budget is spent, rather than
        // relying on the inner reader's own polls to eventually do that for us.
        let mut budget = 4;
        while self.remaining_to_skip > 0 {
            if budget == 0 {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            budget -= 1;

            let chunk = self.remaining_to_skip.min(SKIP_CHUNK_BYTES as u64) as usize;
            let (result, filled) = {
                let this = &mut *self;
                let scratch = this
                    .scratch
                    .as_mut()
                    .expect("scratch exists while bytes remain to skip");
                let mut discard_buf = tokio::io::ReadBuf::new(&mut scratch[..chunk]);
                let result = std::pin::Pin::new(&mut this.inner).poll_read(cx, &mut discard_buf);
                (result, discard_buf.filled().len())
            };
            match result {
                Poll::Ready(Ok(())) => {
                    if filled == 0 {
                        // The underlying stream ended before we finished skipping. This can only
                        // mean the file was truncated to something shorter than what was already
                        // decoded and emitted before the watcher went idle -- `reactivate`'s own
                        // truncation handling is expected to have already reset `file_position`
                        // (and thus never construct this reader) in that case, so reaching this
                        // is unexpected, but returning a clean EOF here rather than looping
                        // forever is the safe fallback either way.
                        self.remaining_to_skip = 0;
                        self.scratch = None;
                        return Poll::Ready(Ok(()));
                    }
                    self.remaining_to_skip -= filled as u64;
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        self.scratch = None;
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
