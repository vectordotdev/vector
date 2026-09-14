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
    AsyncFileInfo, FilePosition, OwnerGeneration, PartialPrefix, PortableFileExt, ReadFrom,
    buffer::{ReadResult, read_until_with_max_size},
};
use vector_common::compression::gzip_multiple_decoder;

const EOF_READ_BACKOFF_MIN: Duration = Duration::from_millis(1);
const EOF_READ_BACKOFF_MAX: Duration = Duration::from_millis(250);

/// How long a file must have been quiet before `should_read`'s throttle paces it down, and how
/// recently it must have been read to bypass that throttle. See `should_read`.
const QUIET_FILE_THROTTLE: Duration = Duration::from_secs(10);

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
/// `Active` is the read state: normally an open file handle is held and reads are attempted
/// against it. Gzip files whose backlog is deliberately skipped are the exception and use a null
/// reader while remaining active for their existing read semantics.
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
        /// Set by `mark_ready_to_read` when a filesystem event named this file, and cleared by the
        /// next read attempt. An explicit flag rather than a back-dated `last_read_attempt`: the
        /// timestamp trick cannot express "read now" when the monotonic clock is younger than the
        /// throttle window, which happens when Vector starts during boot.
        forced_read: bool,
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
    /// Set whenever the reader was repositioned to the start of a *different* file (an identity
    /// change in `update_path`, or a restart after an in-place rewrite). The caller reads it with
    /// `take_reader_restarted` to reset the persisted checkpoint, which otherwise keeps the old
    /// offset until the first line of the new content is acknowledged -- long enough for a restart
    /// to resume past the replacement's prefix.
    reader_restarted: bool,
    /// Bumped for every event that invalidates the content the reader was consuming: an in-place
    /// rewrite, a replacement inode, a truncation.
    ///
    /// The rewind guard compares epochs rather than holding a bare flag, which was wrong in both
    /// directions: a `reactivate` onto a replacement cleared the flag and let the next pass rewind
    /// over emitted data, while a *second* rewrite before the first became fingerprintable was
    /// suppressed as a repeat. Size cannot stand in for it -- a partial rewrite grows as its author
    /// writes, and a later rewrite can land on the same length.
    content_epoch: u64,
    /// The epoch the reader was last rewound for, if it has not been fingerprinted since.
    last_rewind_epoch: Option<u64>,
    /// On-disk length when the reader was last rewound, so a rewrite landing *before* the previous
    /// one became fingerprintable can still be detected.
    ///
    /// Only a shrink below this counts: an append and a second rewrite both grow the file and move
    /// its mtime, so anything less conservative would replay what the first rewrite already emitted.
    rewound_at_len: Option<u64>,
    /// The partial prefix this watcher was last rewound for, when discovery supplied one.
    ///
    /// Complements the length: a rewrite that is still being written only ever *extends* its prefix,
    /// so a prefix that is not an extension is new content -- including one of equal or greater
    /// length, which no size comparison can catch. A second rewrite that happens to start with the
    /// same bytes (a shared log header) remains indistinguishable from growth by content alone.
    rewound_for_prefix: Option<PartialPrefix>,
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
    /// Identifies this watcher as the owner of its checkpoint. Stamped on every line it reads, so a
    /// checkpoint write can be matched against the watcher that produced it; changes when the
    /// watcher is rekeyed, which is an ownership transition.
    generation: OwnerGeneration,
    last_seen: Instant,
    /// When this watcher was first not matched by the current discovery pass. Unlike
    /// `last_seen`, this is not refreshed by later successful matches and therefore gives an
    /// unfindable watcher its full rename/reaping grace period.
    unfindable_since: Option<Instant>,
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
    /// A file excluded by `ignore_before` whose size already matches the resume position starts
    /// `Idle`, opened only briefly for a gzip/identity probe and holding no handle. This is the core
    /// of <https://github.com/vectordotdev/vector/issues/3567>, where such files each held an unused
    /// handle for as long as they existed.
    ///
    /// `idle_on_startup` must be `false` when `FileServer::idle_timeout` is `None`: this startup path
    /// is separate from the runtime `deactivate()` transition, so without the gate `idle_timeout:
    /// null` would not restore the documented always-open behaviour.
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
                        reader_restarted: false,
                        content_epoch: 0,
                        last_rewind_epoch: None,
                        rewound_at_len: None,
                        rewound_for_prefix: None,
                        // Confirmed non-gzip by `gzip_check` above.
                        gzip_read_skipped: false,
                        is_gzip: false,
                        gzip_raw_metadata: None,
                        is_dead: false,
                        generation: file_source_common::next_owner_generation(),
                        last_seen: Instant::now(),
                        unfindable_since: None,
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
                forced_read: false,
            },
            file_position,
            identity: Some((devno, ino)),
            reader_restarted: false,
            content_epoch: 0,
            last_rewind_epoch: None,
            rewound_at_len: None,
            rewound_for_prefix: None,
            gzip_read_skipped,
            is_gzip: gzipped,
            gzip_raw_metadata,
            is_dead: false,
            generation: file_source_common::next_owner_generation(),
            last_seen: ts,
            unfindable_since: None,
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
    /// Restart reading from the beginning after an in-place rewrite (`copytruncate`, or an app that
    /// truncates and rewrites its own log).
    ///
    /// The inode is unchanged, so the offset survives while the contents do not: resuming at it
    /// either seeks past EOF or splices new bytes onto the tail of the old content.
    pub async fn restart_after_rewrite(&mut self) -> io::Result<()> {
        self.restart_after_rewrite_for(None).await
    }

    /// [`Self::restart_after_rewrite`], told which rewrite discovery is currently looking at.
    ///
    /// `observed` is the partial prefix from this pass's fingerprint attempt, when there was one.
    async fn restart_after_rewrite_for(
        &mut self,
        observed: Option<&PartialPrefix>,
    ) -> io::Result<()> {
        // Idempotent *per epoch*, so no caller has to ask first, while a genuine second rewrite --
        // which bumps the epoch -- still repositions the reader.
        if self.rewind_pending() {
            if !self.rewritten_again_since_rewind(observed).await {
                return Ok(());
            }
            // A rewrite on top of the one just rewound for, before it grew enough to fingerprint.
            // Nothing raised the epoch -- the inode never changed -- so raise it here, or the rewind
            // below would immediately re-arm the same guard it just escaped.
            self.content_epoch += 1;
        }
        if self.gzip_read_skipped {
            // A deliberately skipped backlog (`read_from: end`, a resumed checkpoint, `ignore_older`)
            // must stay skipped while the file is still gzip, or restarting replays the history the
            // configuration asked to ignore. But if the rewrite replaced it with a plain file, such a
            // watcher holds a null reader and would never consume the new contents. Re-probe first.
            let file_handle = open_regular_file(&self.path).await?;
            let mut probe = BufReader::new(file_handle);
            if is_gzipped(&mut probe).await? {
                return Ok(());
            }
            self.gzip_read_skipped = false;
        }

        if !self.is_active() {
            // An idle watcher holds no reader, but it does hold the offset `reactivate` will seek
            // to. `reactivate` re-derives a reset only from an inode change or an observed shrink,
            // so a rewrite at least as large as the old offset would resume inside the *new*
            // content and lose or concatenate its prefix. Reset the offset here, where the rewrite
            // is already established, rather than hoping `reactivate` infers it.
            self.file_position = 0;
            self.reader_restarted = true;
            self.last_rewind_epoch = Some(self.content_epoch);
            self.rewound_at_len = tokio::fs::metadata(&self.path)
                .await
                .ok()
                .map(|metadata| metadata.len());
            self.rewound_for_prefix = observed.cloned();
            // The buffered tail belongs to the content this rewrite discarded. Left in place, reaping
            // this watcher before it reactivates would emit those bytes as a record of the new file.
            if let WatcherState::Idle {
                pending_partial_line,
                ..
            } = &mut self.state
            {
                *pending_partial_line = None;
            }
            self.invalidate_idle_bookkeeping();
            return Ok(());
        }

        let file_handle = open_regular_file(&self.path).await?;
        // Identity is re-checked against the descriptor just opened, not against the path: the
        // caller's check and this open are separate syscalls, and a rotation in between would
        // otherwise attach this reader to the replacement while `self.identity` still describes the
        // old inode -- reading the wrong file, and breaking later rename recovery.
        let file_info = file_handle.file_info().await?;
        let opened_identity = (file_info.portable_dev(), file_info.portable_ino());
        if self
            .identity
            .is_some_and(|tracked| tracked != opened_identity)
        {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "file was replaced while reopening it after an in-place rewrite",
            ));
        }

        // Read from the descriptor already opened, not the path: this becomes the new baseline for
        // `shrank_below_reader`, and re-stat-ing the path could pick up a different file.
        let raw_metadata = file_handle.metadata().await.ok();

        let mut reader = BufReader::new(file_handle);
        let gzipped = is_gzipped(&mut reader).await?;
        let reader: Box<dyn AsyncBufRead + Send + Unpin> = if gzipped {
            Box::new(BufReader::new(gzip_multiple_decoder(reader)))
        } else {
            reader.seek(io::SeekFrom::Start(0)).await?;
            Box::new(reader)
        };

        self.is_gzip = gzipped;
        // From the descriptor just read, not a fresh stat of the path: this is the length the reader
        // actually restarted on.
        let rewound_at_len = raw_metadata.as_ref().map(|metadata| metadata.len());
        // Rebase the compressed-size baseline, or every later append that stays below the
        // *pre-rewrite* size is misread as another rewrite and re-emits the records in between.
        self.gzip_raw_metadata = gzipped
            .then(|| raw_metadata.map(|metadata| (metadata.len(), metadata.modified().ok())))
            .flatten();
        self.file_position = 0;
        self.reader_restarted = true;
        self.last_rewind_epoch = Some(self.content_epoch);
        self.rewound_at_len = rewound_at_len;
        self.rewound_for_prefix = observed.cloned();
        self.state = WatcherState::Active {
            reader,
            reached_eof: false,
            last_read_attempt: Instant::now(),
            last_read_success: Instant::now(),
            read_retry_delay: EOF_READ_BACKOFF_MIN,
            buf: BytesMut::new(),
            forced_read: false,
        };
        Ok(())
    }

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
            // A different inode: the persisted checkpoint still names the old file's offset.
            self.reader_restarted = true;
            // The reader is already at the replacement's start. Without this, a fingerprint that only
            // completes later is read as an in-place rewrite and rewinds it a second time, replaying
            // whatever it emitted in between.
            self.last_rewind_epoch = Some(self.content_epoch);
            self.rewound_at_len = None;
            self.rewound_for_prefix = None;
            self.state = WatcherState::Active {
                reader: new_reader,
                reached_eof: false,
                last_read_attempt: Instant::now(),
                last_read_success: Instant::now(),
                read_retry_delay: EOF_READ_BACKOFF_MIN,
                buf: BytesMut::new(),
                forced_read: false,
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

    /// Whether this watcher is in the active read state.
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(self.state, WatcherState::Active { .. })
    }

    /// Whether the active state currently owns a real file handle. A gzip watcher configured to
    /// skip an already-compressed backlog is logically active but uses a null reader instead.
    #[inline]
    pub fn holds_file_handle(&self) -> bool {
        self.is_active() && !self.gzip_read_skipped
    }

    /// Whether `path` denotes this watcher's logical path. Notify reports absolute paths while a
    /// glob provider may retain a relative path, so compare both after applying the same CWD.
    pub fn matches_path(&self, path: &Path, cwd: Option<&Path>) -> bool {
        let path = crate::absolutize(path, cwd);
        crate::absolutize(&self.path, cwd) == path
            || self.canonical_path.as_deref() == Some(path.as_path())
    }

    /// Whether the file has shrunk below what this reader consumed -- an in-place rewrite rather than
    /// an append.
    ///
    /// Gzip needs a different baseline: `file_position` counts *decoded* bytes against a *compressed*
    /// size, so a direct comparison makes every compressible file look truncated. `false` when the
    /// size cannot be read or a gzip watcher has no baseline; neither is evidence of a rewrite.
    ///
    /// Owned future: borrowing `&self` across the `await` makes `FileServer::run` non-`Send`.
    pub fn shrank_below_reader(&self) -> impl std::future::Future<Output = bool> + Send + 'static {
        let path = self.path.clone();
        let is_gzip = self.is_gzip;
        let gzip_raw_metadata = self.gzip_raw_metadata;
        let file_position = self.file_position;
        async move {
            let Ok(metadata) = tokio::fs::metadata(&path).await else {
                return false;
            };
            if is_gzip {
                gzip_raw_metadata.is_some_and(|(raw_len, _)| metadata.len() < raw_len)
            } else {
                metadata.len() < file_position
            }
        }
    }

    /// Whether the file was rewritten *again* since the reader was rewound, so the rewind guard
    /// must not suppress another restart.
    ///
    /// Two independent signals, because neither alone is enough:
    ///
    /// - the prefix is not an extension of the one rewound for. Catches a rewrite of any length,
    ///   including a longer one, and works for gzip since the bytes compared are the decoded ones
    ///   the strategy read. Only available when discovery supplied a prefix this pass.
    /// - the file shrank below the length rewound at. Catches a rewrite whose prefix happens to
    ///   start with the same bytes -- a shared log header -- which no content comparison can see.
    ///   Skipped for gzip, where the recorded raw length and a compressed size are not comparable.
    ///
    /// A second rewrite that both starts with the same bytes and is no shorter is indistinguishable
    /// from the first still being written, and is caught when the fingerprint completes instead.
    /// Owned future for the same reason as [`Self::shrank_below_reader`]: borrowing `&self` across
    /// the `await` makes `FileServer::run` non-`Send`.
    fn rewritten_again_since_rewind(
        &self,
        observed: Option<&PartialPrefix>,
    ) -> impl std::future::Future<Output = bool> + Send + 'static {
        let prefix_differs = match (observed, self.rewound_for_prefix.as_ref()) {
            (Some(observed), Some(rewound_for)) => !observed.continues(rewound_for),
            _ => false,
        };
        let shrank = self.shrank_below_rewind();
        async move { prefix_differs || shrank.await }
    }

    /// Whether the file shrank below the length it was last rewound at.
    ///
    /// `false` without a recorded length, when the size cannot be read, or for gzip: the recorded
    /// length is raw bytes for a plain file, and mixing it with a compressed size would read a
    /// well-compressing rewrite as a shrink.
    ///
    /// Owned future for the same reason as [`Self::shrank_below_reader`]: borrowing `&self` across
    /// the `await` makes `FileServer::run` non-`Send`.
    fn shrank_below_rewind(&self) -> impl std::future::Future<Output = bool> + Send + 'static {
        let path = self.path.clone();
        let rewound_at_len = (!self.is_gzip).then_some(self.rewound_at_len).flatten();
        async move {
            let Some(rewound_at_len) = rewound_at_len else {
                return false;
            };
            tokio::fs::metadata(&path)
                .await
                .is_ok_and(|metadata| metadata.len() < rewound_at_len)
        }
    }

    /// Whether this watcher reads through a gzip decoder. Callers comparing `get_file_position()`
    /// against a file's on-disk size must check this first: the position counts *decoded* bytes while
    /// the size is *compressed* bytes, so any compressible file looks "shrunk" without being
    /// truncated.
    #[inline]
    pub fn is_gzip(&self) -> bool {
        self.is_gzip
    }

    #[inline]
    pub fn is_idle(&self) -> bool {
        matches!(self.state, WatcherState::Idle { .. })
    }

    pub fn set_file_findable(&mut self, f: bool) {
        if f {
            self.mark_found();
            // Only the glob pass passes `true`, and it visits only paths the `PathsProvider`
            // yielded, so the tracked inode is back inside the include patterns.
            self.path_outside_glob = false;
        } else {
            self.findable = false;
            if self.unfindable_since.is_none() {
                self.unfindable_since = Some(Instant::now());
            }
        }
    }

    /// Record that the file was just observed, without asserting glob membership. Notify event
    /// paths are not glob-filtered, so a rotated file still emitting events from outside the glob
    /// must keep its `path_outside_glob` exemption.
    pub fn mark_found(&mut self) {
        self.findable = true;
        self.last_seen = Instant::now();
        self.unfindable_since = None;
    }

    /// Mark the start of a discovery pass without starting the unfindable grace period yet.
    /// The grace period begins only after the pass confirms that this watcher was not found.
    pub fn prepare_for_discovery(&mut self) {
        self.findable = false;
    }

    /// Start the unfindable grace period for a watcher still missing after discovery. Existing
    /// timestamps are preserved so repeated passes do not extend the grace period.
    pub fn finish_discovery(&mut self) {
        if !self.findable && self.unfindable_since.is_none() {
            self.unfindable_since = Some(Instant::now());
        }
    }

    /// Mark this watcher as still live at a path outside the configured include patterns. This
    /// is used after an idle watcher is found by identity following a rotation, so subsequent
    /// glob passes do not make `poll_idle_watchers` abandon the rotated inode.
    pub fn mark_path_outside_glob(&mut self) {
        self.path_outside_glob = true;
        self.last_seen = Instant::now();
    }

    /// Stop treating this watcher as identity-verified outside the configured include patterns.
    /// This is needed when the last known outside-glob path no longer contains the tracked file.
    pub fn clear_path_outside_glob(&mut self) {
        self.path_outside_glob = false;
    }

    #[inline]
    pub fn path_is_outside_glob(&self) -> bool {
        self.path_outside_glob
    }

    /// Check whether the current path still resolves to the tracked file identity. This is a
    /// single-file check used to avoid rescanning an entire archive tree on every polling pass
    /// for an outside-glob idle watcher; a tree scan is only needed once this path disappears or
    /// resolves to a replacement.
    /// Whether the tracked file is definitely no longer at this watcher's path -- either nothing is
    /// there, or a different inode is. Distinguished from a merely *unreadable* path (permission,
    /// sharing, or other I/O error), which leaves identity indeterminate: treating that as deletion
    /// would abandon a rotated file that is only temporarily inaccessible.
    pub fn tracked_file_is_gone(&self) -> impl std::future::Future<Output = bool> + Send + 'static {
        let path = self.path.clone();
        let identity = self.identity;
        async move {
            if path_is_absent(&path).await {
                return true;
            }
            match (path_identity(&path).await, identity) {
                (Some(current), Some(tracked)) => current != tracked,
                _ => false,
            }
        }
    }

    /// Whether the reader was repositioned onto different content since this was last called,
    /// clearing the flag. The caller resets the persisted checkpoint in response: it otherwise keeps
    /// the pre-restart offset until the first new line is acknowledged, and a restart in that window
    /// resumes past the new content's prefix.
    pub fn take_reader_restarted(&mut self) -> bool {
        std::mem::take(&mut self.reader_restarted)
    }

    pub fn path_has_tracked_identity(
        &self,
    ) -> impl std::future::Future<Output = bool> + Send + 'static {
        self.candidate_has_tracked_identity(self.path.clone())
    }

    /// As [`Self::path_has_tracked_identity`], but for a path the caller discovered rather than the
    /// one this watcher currently holds.
    ///
    /// The two differ when an alias disappears: the watcher's own spelling stops resolving while
    /// another spelling of the same inode still does, and deciding on the wrong one preserves a
    /// reader offset into content that was rewritten.
    pub fn candidate_has_tracked_identity(
        &self,
        candidate: PathBuf,
    ) -> impl std::future::Future<Output = bool> + Send + 'static {
        let identity = self.identity;
        async move {
            let Some(identity) = identity else {
                return false;
            };
            path_identity(&candidate).await == Some(identity)
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

    /// The generation stamped on every line this watcher reads.
    pub fn generation(&self) -> OwnerGeneration {
        self.generation
    }

    /// Take a fresh generation, retiring the one lines already in flight were stamped with.
    ///
    /// Called wherever the reader is repositioned onto different content -- a rekey onto a new
    /// fingerprint, or a reset to zero under the same one. Both discard what the reader had
    /// consumed, so an acknowledgement still travelling for that content must not be allowed to
    /// move the checkpoint back past the reset.
    pub fn take_new_generation(&mut self) -> OwnerGeneration {
        self.generation = file_source_common::next_owner_generation();
        self.generation
    }

    pub fn set_dead(&mut self) {
        self.is_dead = true;
    }

    pub fn dead(&self) -> bool {
        self.is_dead
    }

    /// Whether the reader was already restarted for the rewrite currently on disk.
    ///
    /// A fingerprint that keeps failing (a file rewritten to fewer lines than `FirstLinesChecksum`
    /// needs returns `UnexpectedEof` on every pass) must not restart the reader again: it would
    /// re-emit whatever was already consumed on each reconciliation.
    fn rewind_pending(&self) -> bool {
        self.last_rewind_epoch == Some(self.content_epoch)
    }

    /// Bring the reader into line with the rewrite discovery is currently looking at.
    ///
    /// The single entry point for all three discovery branches, so the decision cannot be made
    /// differently in each: repairing one branch and missing its twin has been this code's most
    /// persistent defect. `fingerprint_complete` says whether this pass produced a fingerprint --
    /// which is what ends a rewrite -- and `observed` carries the partial prefix when it did not.
    ///
    /// Callers no longer test [`Self::rewind_pending`] first. That test is an early exit *inside*
    /// the rewind, where it can also see that the content changed again; hoisting it into the
    /// callers made the second-rewrite check unreachable.
    pub async fn reconcile_rewrite(
        &mut self,
        fingerprint_complete: bool,
        observed: Option<&PartialPrefix>,
    ) -> io::Result<()> {
        // The rewind first, while the guard still stands: it is what says the reader is already at
        // the start of this content. Lowering the guard first makes the call below rewind a second
        // time and replay everything emitted since.
        let outcome = self.restart_after_rewrite_for(observed).await;
        if fingerprint_complete {
            self.fingerprint_completed();
        }
        outcome
    }

    /// The file fingerprinted again, which is what ends a rewrite.
    ///
    /// The only way the guard is lowered. Call it wherever a fingerprint succeeds *for this watcher*
    /// -- not from a pass epilogue, since a targeted pass does not observe every watcher. Clearing is
    /// tied to the epoch, so a completion that arrives after a further rewrite cannot release it.
    pub fn fingerprint_completed(&mut self) {
        if self.rewind_pending() {
            self.last_rewind_epoch = None;
            self.rewound_at_len = None;
            self.rewound_for_prefix = None;
        }
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

    /// Force the next `check_for_new_data` to report a change.
    ///
    /// Call after a failed `reactivate()`: `check_for_new_data` has already recorded the size/mtime
    /// it observed, so an unchanged file would compare equal on the next poll and never retry,
    /// stranding the watcher `Idle` with unread data.
    ///
    /// A separate flag rather than clobbering `last_known_size`: a fabricated baseline would make a
    /// genuine truncation before the next poll undetectable. No-op unless `Idle`.
    pub fn invalidate_idle_bookkeeping(&mut self) {
        if let WatcherState::Idle { force_recheck, .. } = &mut self.state {
            *force_recheck = true;
        }
    }

    /// Promote an `Idle` watcher back to `Active`: reopen, re-detect gzip, and seek to
    /// `file_position`. No-op if already `Active`.
    ///
    /// The offset is reset to 0 in two cases, because seeking a stale offset into different content
    /// either skips the new file's opening bytes or reads nothing until it grows past that point:
    ///
    /// - A confirmed identity (`self.identity` is `Some`) that no longer matches: the file was
    ///   replaced in place, which `discover`'s rename detection cannot see.
    /// - `truncated_while_idle`, a latch set by `check_for_new_data` across the whole idle period.
    ///   It must be a latch: a truncate seen by one poll and a refill seen by a later one look
    ///   exactly like ordinary growth by the time `reactivate` looks.
    ///
    /// An identity of `None` is *unconfirmed*, not evidence of replacement, and must not reset:
    /// `file_position` there holds the file's size as of discovery, so resetting re-reads all the
    /// content `ignore_older` deliberately skipped.
    ///
    /// **Two limitations**, both inherent to polling rather than to idle-handle-closing. With no
    /// identity available, a first append cannot be told from a same-path replacement whose content
    /// fingerprints identically, so growth is assumed. And a truncate *and* regrowth landing between
    /// two polls leaves nothing on disk to detect afterwards -- `polling` mode has the same blind
    /// spot for active files.
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
            // The persisted checkpoint still names the previous file's offset; the caller resets it
            // once it sees this flag. Without that, a restart before the first line of the new
            // content is acknowledged resumes past that content's prefix.
            self.reader_restarted = true;
            // A replacement inode is a new content epoch, and this reset *is* its rewind: clearing
            // instead would let the next pass rewind again over what has been emitted.
            self.content_epoch += 1;
            self.last_rewind_epoch = Some(self.content_epoch);
            self.rewound_at_len = None;
            self.rewound_for_prefix = None;
        } else if truncated_while_idle || truncated_at_reactivation {
            debug!(
                message = "Idle watcher's file was truncated in place while idle (same \
                           identity). Resuming from the start rather than seeking past \
                           stale, since-invalidated content.",
                path = ?self.path,
            );
            file_position = 0;
            gzip_read_skipped = false;
            // Same reasoning as above: the offset the checkpoint holds no longer exists.
            self.reader_restarted = true;
            // Truncation likewise starts a new epoch, and this reset is its rewind.
            self.content_epoch += 1;
            self.last_rewind_epoch = Some(self.content_epoch);
            self.rewound_at_len = None;
            self.rewound_for_prefix = None;
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
            forced_read: false,
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

        // `buf` holds bytes already counted into `file_position` for a line with no delimiter yet.
        // Going idle discards `buf`, so `file_position` is rewound behind them -- otherwise
        // `reactivate` resumes *after* bytes that are still on disk and never reads them. Saturating,
        // because a mid-read truncation can leave the buffer longer than the position.
        //
        // The bytes themselves are kept in `pending_partial_line` with their starting offset, so
        // `FileServer` can salvage them if this watcher is reaped while still `Idle`.
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
            last_read_attempt,
            forced_read,
            ..
        } = &mut self.state
        {
            *last_read_attempt = Instant::now();
            // The forced read has happened; further reads are paced normally again.
            *forced_read = false;
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
            read_retry_delay,
            forced_read,
            ..
        } = &mut self.state
        {
            *reached_eof = false;
            *read_retry_delay = EOF_READ_BACKOFF_MIN;
            // An explicit flag, not a back-dated `last_read_attempt`. Clearing `reached_eof` alone
            // does not get past `should_read`'s quiet-file throttle, and back-dating cannot express
            // "read now" at all when the monotonic clock is younger than the throttle window --
            // which is the case when Vector starts during boot. Worse, `checked_sub` then falls back
            // to *now*, making the throttle reject the very read the event was supposed to force.
            *forced_read = true;
        }
    }

    #[inline]
    pub fn should_read(&self) -> bool {
        let WatcherState::Active {
            reached_eof,
            last_read_attempt,
            last_read_success,
            read_retry_delay,
            forced_read,
            ..
        } = &self.state
        else {
            // Idle watchers hold no reader; `FileServer` polls them via
            // `check_for_new_data` on the glob-rescan cadence instead.
            return false;
        };

        // A filesystem event named this file, so neither the EOF backoff nor the quiet-file
        // throttle applies: both exist to pace *unprompted* polling.
        if *forced_read {
            return true;
        }

        if *reached_eof && last_read_attempt.elapsed() < *read_retry_delay {
            return false;
        }

        last_read_success.elapsed() < QUIET_FILE_THROTTLE
            || last_read_attempt.elapsed() > QUIET_FILE_THROTTLE
    }

    #[inline]
    pub fn last_seen(&self) -> Instant {
        self.last_seen
    }

    #[inline]
    pub fn unfindable_for(&self) -> Duration {
        self.unfindable_since
            .map_or(Duration::ZERO, |since| since.elapsed())
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

/// Whether the file at `path` is known to be gone, as opposed to merely unreadable. A permission,
/// sharing, or other I/O error leaves identity *indeterminate*, which must not be mistaken for
/// deletion. `metadata` (not `symlink_metadata`) is used so that a symlink whose target was deleted
/// counts as gone: the link itself still resolves as an entry, but the tracked file no longer exists.
pub(crate) async fn path_is_absent(path: &std::path::Path) -> bool {
    match tokio::fs::metadata(path).await {
        Err(error) => error.kind() == io::ErrorKind::NotFound,
        // A directory or other non-regular entry at this path means the tracked file is gone even
        // though something still answers here, so the watcher must not keep its outside-glob
        // exemption and the long `rotate_wait` grace that comes with it.
        Ok(metadata) => !metadata.is_file(),
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
