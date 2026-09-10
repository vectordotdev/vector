use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use std::{
    io::{self, SeekFrom},
    path::PathBuf,
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
        /// Last known size of the file, as of the last successful stat.
        last_known_size: u64,
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
    findable: bool,
    state: WatcherState,
    file_position: FilePosition,
    /// Device and inode of the underlying file, once known. `None` only for a
    /// watcher that started `Idle` and has never been opened: there is no
    /// portable way to learn a file's identity without a handle
    /// (`GetFileInformationByHandle` is required even on Windows), so we
    /// can't populate this until the first `reactivate`/`update_path` open.
    /// Callers that need identity to detect renames (`update_path`) already
    /// treat "identity unknown" the same as "identity changed", which is the
    /// correct, safe behavior: it forces a fresh open rather than risking a
    /// stale-offset read against the wrong file.
    identity: Option<(u64, u64)>,
    /// Whether the current gzip stream (if any) was deliberately left unread, rather than being
    /// positioned wherever it is because we've actually decoded up to that point. Distinct from
    /// "`file_position == 0`," which is ambiguous: `0` also means "haven't decoded anything yet
    /// because we're about to start at the beginning," a completely different situation this flag
    /// exists so `reactivate` can tell apart. Set whenever `FileWatcher::new`/`reactivate`/
    /// `update_path` choose a null reader over the real gzip decoder (an already-compressed file
    /// with `read_from: end`, or with `read_from: checkpoint` pointing at a non-zero -- and thus
    /// unresumable -- gzip byte offset); cleared whenever they instead install a real decoder.
    /// Without this, an idle gzip watcher skipped via `read_from: end` (file position ends up `0`,
    /// same as "start of file") gets misread on reactivation as "never started decoding, so start
    /// decoding from the beginning," installing a real decoder and emitting the entire backlog
    /// that `read_from: end` was supposed to skip -- even though nothing about a mere mtime bump
    /// means the file is safe to resume decoding (gzip streams can't be resumed from an arbitrary
    /// point anyway, which is exactly why this was skipped in the first place).
    gzip_read_skipped: bool,
    is_dead: bool,
    last_seen: Instant,
    max_line_bytes: usize,
    line_delimiter: Bytes,
}

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
    /// is *not* opened at all: the watcher starts in the `Idle` state, holding
    /// no file handle. This is the core of the fix for
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
        // Cheap stat-only pass first. This lets us avoid ever calling
        // `File::open` for files that are both old (per `ignore_before`) and
        // fully read already (size == checkpointed position), which is
        // exactly the "12,000 idle files" scenario from #3567.
        let stat = tokio::fs::metadata(&path).await?;
        let modified_time = stat.modified().ok();
        let too_old =
            if let (Some(ignore_before), Some(modified_time)) = (ignore_before, modified_time) {
                DateTime::<Utc>::from(modified_time) < ignore_before
            } else {
                false
            };

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
            if let Some(false) = gzip_check {
                debug!(
                    message = "Starting file watcher in idle state; no unread data and file is older than `ignore_older`.",
                    ?path,
                    file_position = %stat.len(),
                );
                return Ok(FileWatcher {
                    path,
                    findable: true,
                    state: WatcherState::Idle {
                        last_known_size: stat.len(),
                        last_known_mtime: modified_time,
                        idle_since: Instant::now(),
                        truncated_while_idle: false,
                        force_recheck: false,
                        pending_partial_line: None,
                        reached_eof: true,
                    },
                    file_position: stat.len(),
                    // We haven't kept the file open, so we don't yet know its
                    // dev/inode; the first reopen (triggered by the
                    // idle->active transition, or by `update_path` on a
                    // rename) will populate it.
                    identity: None,
                    // Confirmed non-gzip by `gzip_check` above.
                    gzip_read_skipped: false,
                    is_dead: false,
                    last_seen: Instant::now(),
                    max_line_bytes,
                    line_delimiter,
                });
            }
            // Either it's gzip (needs the full open+decode path below to get
            // position 0 vs EOF right) or the file vanished/became
            // unreadable between the stat above and `peek_is_gzipped`'s open
            // (`gzip_check` is `None`) -- either way, fall through to the
            // normal open path, which handles both correctly (and will
            // surface a real error for the latter case).
        }

        let f = File::open(&path).await?;
        let file_info = f.file_info().await?;
        let (devno, ino) = (file_info.portable_dev(), file_info.portable_ino());

        #[cfg(unix)]
        let metadata = file_info;
        #[cfg(windows)]
        let metadata = f.metadata().await?;

        let mut reader = BufReader::new(f);

        let gzipped = is_gzipped(&mut reader).await?;

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

        let ts = metadata
            .modified()
            .ok()
            .and_then(|mtime| mtime.elapsed().ok())
            .and_then(|diff| Instant::now().checked_sub(diff))
            .unwrap_or_else(Instant::now);

        Ok(FileWatcher {
            path,
            findable: true,
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

        let file_handle = File::open(&path).await?;

        let file_info = file_handle.file_info().await?;
        let new_identity = (file_info.portable_dev(), file_info.portable_ino());
        if Some(new_identity) != self.identity {
            let mut reader = BufReader::new(File::open(&path).await?);
            let gzipped = is_gzipped(&mut reader).await?;
            let new_reader: Box<dyn AsyncBufRead + Send + Unpin> = if gzipped {
                if self.file_position != 0 {
                    self.gzip_read_skipped = true;
                    Box::new(null_reader())
                } else {
                    self.gzip_read_skipped = false;
                    Box::new(BufReader::new(gzip_multiple_decoder(reader)))
                }
            } else {
                self.gzip_read_skipped = false;
                reader.seek(io::SeekFrom::Start(self.file_position)).await?;
                Box::new(reader)
            };

            let file_info = file_handle.file_info().await?;
            self.identity = Some((file_info.portable_dev(), file_info.portable_ino()));

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
        self.path = path;
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
        // this that clobbered it with a sentinel was wrong. So the size/mtime comparison here is
        // always a genuine one, and `force_recheck` only affects whether `changed` is reported as
        // `true` on top of that; it never suppresses or replaces the real shrink check below.
        let sizes_or_mtimes_differ = new_size != *last_known_size || new_mtime != *last_known_mtime;
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
        if new_size < *last_known_size {
            *truncated_while_idle = true;
            // The pre-truncation offset/bytes no longer correspond to anything on disk.
            *pending_partial_line = None;
        }

        // Always keep our idle bookkeeping current so that a subsequent
        // truncation-then-refill (or vice versa) is still detected relative
        // to what we most recently observed.
        *last_known_size = new_size;
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
    /// A `self.identity` of `None`, by contrast, means this watcher has *never* opened the file:
    /// it started `Idle` straight out of `FileWatcher::new`'s startup fast path for an
    /// `ignore_older`-excluded file, without ever confirming any identity at all. That's not
    /// evidence of a replacement -- it's simply "unconfirmed" -- so unlike a real identity
    /// mismatch, it must not reset `file_position`: doing so would re-read a file's entire old
    /// content (which `ignore_older` deliberately skipped) the very first time it receives new
    /// data, since `file_position` in that case holds the file's size *as of discovery*, not a
    /// checkpoint from a previous read. This reactivation is simply the first time we're
    /// confirming identity, not a change of it.
    ///
    /// **Known limitation**, an accepted trade-off of the startup fast path rather than something
    /// this function can fix on its own: because a never-opened watcher has no identity to compare
    /// against, this function cannot distinguish "an `ignore_older`-excluded file received its
    /// first append" from "that file was replaced (not renamed) by a different, larger file at
    /// the same path, whose content happens to fingerprint identically to the old one under the
    /// default first-line-only strategy" before its first reactivation. The former (by far the
    /// common case) requires resuming from the retained `file_position`; the latter would need
    /// resuming from 0. Since a replacement can't be told apart from a growth here, and 0 would be
    /// wrong far more often (re-sending the entire skipped backlog on every single first
    /// reactivation, defeating the point of the fast path), this function assumes growth. Getting
    /// this case exactly right would require either opening the file at startup after all
    /// (eliminating the fast path this exists to provide) or a fingerprinting strategy strong
    /// enough to make same-content-prefix collisions practically impossible, neither of which is
    /// a change this function is positioned to make locally.
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

        let f = File::open(&self.path).await?;
        let file_info = f.file_info().await?;
        let new_identity = (file_info.portable_dev(), file_info.portable_ino());
        let identity_changed = matches!(self.identity, Some(old) if old != new_identity);
        self.identity = Some(new_identity);

        let mut reader = BufReader::new(f);
        let gzipped = is_gzipped(&mut reader).await?;

        // Also fall back to a direct, final check against the file we just opened: this covers
        // reactivation paths that don't go through `check_for_new_data` first (e.g. a caller that
        // calls `reactivate` directly, as some tests do), where `truncated_while_idle` was never
        // given a chance to latch. Skipped for gzip: `self.file_position` there counts decompressed
        // bytes, not the compressed on-disk size `metadata().len()` reports, so comparing the two
        // would misfire as "truncated" on ordinary compressible content. `truncated_while_idle`
        // (raw on-disk size vs. raw on-disk size, from `check_for_new_data`) still catches a real
        // gzip truncation correctly.
        let truncated_at_reactivation = !gzipped
            && reader
                .get_ref()
                .metadata()
                .await
                .is_ok_and(|m| m.len() < self.file_position);
        if identity_changed {
            debug!(
                message = "Idle watcher's file identity changed on reactivation; \
                           the file at this path was replaced while idle. Resuming \
                           from the start rather than the stale checkpoint offset.",
                path = ?self.path,
            );
            self.file_position = 0;
            // A different file is now at this path: whatever was true of the old file's gzip
            // stream (skipped or not) says nothing about this one, which we haven't looked at
            // yet. Clear the flag so the check below falls through to installing a real decoder,
            // the same as `FileWatcher::new` would for a freshly-discovered gzip file.
            self.gzip_read_skipped = false;
        } else if truncated_while_idle || truncated_at_reactivation {
            debug!(
                message = "Idle watcher's file was truncated in place while idle (same \
                           identity). Resuming from the start rather than seeking past \
                           stale, since-invalidated content.",
                path = ?self.path,
            );
            self.file_position = 0;
            // Same reasoning as the identity-changed case above: the truncated content
            // invalidates whatever "skipped" state applied to the pre-truncation stream.
            self.gzip_read_skipped = false;
        }

        let (reader, file_position, gzip_read_skipped): (
            Box<dyn AsyncBufRead + Send + Unpin>,
            FilePosition,
            bool,
        ) = if gzipped {
            if self.gzip_read_skipped || self.file_position != 0 {
                // Either this gzip stream was deliberately left unread (e.g. `read_from: end`
                // skipped it entirely, leaving `file_position` at `0`) rather than actually
                // decoded up to `file_position` -- a mtime/size change alone doesn't make it safe
                // to resume, since gzip streams can't be resumed from an arbitrary offset
                // regardless, which is exactly why this was skipped in the first place -- or
                // `file_position` is genuinely non-zero, which is the pre-existing "can't resume
                // a gzip stream from an arbitrary byte offset" case. Either way, behave like the
                // "already read, ignore" case `FileWatcher::new` uses for gzip + checkpoint.
                (Box::new(null_reader()), self.file_position, true)
            } else {
                (
                    Box::new(BufReader::new(gzip_multiple_decoder(reader))),
                    0,
                    false,
                )
            }
        } else {
            // Propagate a seek failure instead of pretending it succeeded: swallowing it (an
            // earlier version of this used `.unwrap_or(self.file_position)`) would report the
            // stale checkpoint offset as the new position while the reader's actual cursor stays
            // wherever `is_gzipped`'s `fill_buf` peek left it -- typically near the start of the
            // file, not `self.file_position` -- so the watcher would go `Active` and immediately
            // start reading from the wrong place: duplicating old content under the wrong
            // offsets, or skipping data, depending on which is larger. Letting this error surface
            // instead leaves the watcher `Idle` (this function's caller, `poll_idle_watchers`,
            // already retries via `invalidate_idle_bookkeeping` on any `Err`), which is a strictly
            // safer outcome than silently reading from an unknown position.
            let pos = reader.seek(SeekFrom::Start(self.file_position)).await?;
            (Box::new(reader), pos, false)
        };
        self.gzip_read_skipped = gzip_read_skipped;

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
        self.file_position = rewound_file_position;

        // Best-effort stat so our idle bookkeeping starts accurate; if this
        // fails (e.g. file was just deleted) fall back to what we already
        // know from `file_position`, which will simply cause the next
        // `check_for_new_data` poll to treat any discrepancy as "changed",
        // which is a safe (if slightly wasteful) default.
        let (last_known_size, last_known_mtime) = match tokio::fs::metadata(&self.path).await {
            Ok(stat) => (stat.len(), stat.modified().ok()),
            Err(_) => (self.file_position, None),
        };

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
            force_recheck: false,
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
                if !self.file_findable() {
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

async fn is_gzipped(r: &mut BufReader<File>) -> io::Result<bool> {
    let header_bytes = r.fill_buf().await?;
    // WARN: The paired `BufReader::consume` is not called intentionally. If we
    // do we'll chop a decent part of the potential gzip stream off.
    Ok(header_bytes.starts_with(GZIP_MAGIC))
}

/// Cheaply check whether a file starts with the gzip magic bytes, opening and
/// immediately closing it rather than keeping a `FileWatcher`-owned handle
/// around. Used by the `too_old` fast path in `FileWatcher::new` to decide
/// whether a file can safely start `Idle` without going through the full
/// open-and-decode path below: unlike a fully-open `FileWatcher`, this never
/// outlives the single `.await` here, so it doesn't reintroduce the
/// long-lived handle the `Idle` state exists to avoid.
///
/// Returns `Ok(None)` if the file couldn't be opened or read (e.g. deleted or
/// permissions changed since the earlier `fs::metadata` call); callers should
/// treat that the same as "unknown, fall back to the full open path" rather
/// than assuming either gzip or not.
async fn peek_is_gzipped(path: &std::path::Path) -> Option<bool> {
    let f = File::open(path).await.ok()?;
    let mut reader = BufReader::new(f);
    is_gzipped(&mut reader).await.ok()
}

fn null_reader() -> impl AsyncBufRead {
    io::Cursor::new(Vec::new())
}
