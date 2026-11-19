use async_compression::tokio::bufread::GzipDecoder;
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use std::{
    io::{self, SeekFrom},
    path::PathBuf,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncSeekExt, BufReader, ReadBuf};
use tracing::debug;
use vector_common::constants::GZIP_MAGIC;

use file_source_common::{
    AsyncFile, Clock, FilePosition, FileSystem, MonotonicInstant, OsFileSystem, ReadFrom,
    SystemClock,
    buffer::{ReadResult, read_until_with_max_size},
};
use vector_common::compression::gzip_multiple_decoder;

const EOF_READ_BACKOFF_MIN: Duration = Duration::from_millis(1);
const EOF_READ_BACKOFF_MAX: Duration = Duration::from_millis(250);

#[cfg(test)]
mod tests;

/// Enum-based reader that preserves type information for metadata access.
///
/// We use an enum instead of `Box<dyn AsyncBufRead>` to preserve type information,
/// allowing us to call `BufReader::get_ref()` on the `Plain` variant to access the
/// underlying `File` for metadata (e.g., current file size via `file.metadata()`).
///
/// Alternative approaches considered:
/// - `Box<dyn AsyncBufRead>`: Erases type info, can't call `get_ref()` to access File
/// - `try_clone()` to store separate File handle: Doubles fd usage via `dup()` syscall
/// - Raw fd with `fstat()`: Works but requires unsafe code
///
/// The enum approach has zero extra fd overhead - we access the same File owned by
/// `BufReader` through `get_ref()`. This is critical for accurately tracking
/// `bytes_unread` even after file deletion (the fd remains valid).
enum FileReader<F: AsyncFile = tokio::fs::File> {
    /// Plain file reader - we can access the File via `get_ref()` for metadata
    Plain(BufReader<F>),
    /// Gzipped file reader - no meaningful file position tracking
    /// Boxed to reduce enum size (this variant is much larger than others)
    Gzipped(Box<BufReader<GzipDecoder<BufReader<F>>>>),
    /// Null reader for skipped files
    Null(io::Cursor<Vec<u8>>),
}

impl<F: AsyncFile> FileReader<F> {
    /// Get the current file size by accessing the underlying File.
    /// Returns None for metadata errors, or gzipped or null readers where file size isn't meaningful.
    /// This works even after the file has been deleted (on Unix) since the fd is still open.
    async fn file_size(&self) -> Option<u64> {
        match self {
            FileReader::Plain(reader) => reader.get_ref().metadata().await.ok().map(|m| m.len),
            FileReader::Gzipped(_) | FileReader::Null(_) => None,
        }
    }
}

impl<F: AsyncFile> AsyncRead for FileReader<F> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            FileReader::Plain(r) => Pin::new(r).poll_read(cx, buf),
            FileReader::Gzipped(r) => Pin::new(r).poll_read(cx, buf),
            FileReader::Null(r) => Pin::new(r).poll_read(cx, buf),
        }
    }
}

impl<F: AsyncFile> AsyncBufRead for FileReader<F> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        match self.get_mut() {
            FileReader::Plain(r) => Pin::new(r).poll_fill_buf(cx),
            FileReader::Gzipped(r) => Pin::new(r).poll_fill_buf(cx),
            FileReader::Null(r) => Pin::new(r).poll_fill_buf(cx),
        }
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        match self.get_mut() {
            FileReader::Plain(r) => Pin::new(r).consume(amt),
            FileReader::Gzipped(r) => Pin::new(r).consume(amt),
            FileReader::Null(r) => Pin::new(r).consume(amt),
        }
    }
}

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

/// Information about a file when it is unwatched.
/// Used for metric emission when Vector stops watching a file for any reason:
/// - File deleted
/// - File rotated and old file removed
/// - Inode changed (file replaced)
/// - `rotate_wait` timeout
#[derive(Debug, Clone)]
pub struct FileUnwatchInfo {
    /// The path of the file
    pub path: PathBuf,
    /// Number of unread bytes, or None when the count cannot be determined.
    pub bytes_unread: Option<u64>,
    /// Whether the file reached EOF before being unwatched
    pub reached_eof: bool,
}

/// The `FileWatcher` struct defines the polling based state machine which reads
/// from a file path, transparently updating the underlying file descriptor when
/// the file has been rolled over, as is common for logs.
///
/// The `FileWatcher` is expected to live for the lifetime of the file
/// path. `FileServer` is responsible for clearing away `FileWatchers` which no
/// longer exist.
pub struct FileWatcher<FS: FileSystem = OsFileSystem> {
    pub path: PathBuf,
    fs: FS,
    findable: bool,
    reader: FileReader<FS::File>,
    file_position: FilePosition,
    devno: u64,
    inode: u64,
    is_dead: bool,
    reached_eof: bool,
    last_read_attempt: MonotonicInstant,
    last_read_success: MonotonicInstant,
    read_retry_delay: Duration,
    last_seen: MonotonicInstant,
    max_line_bytes: usize,
    line_delimiter: Bytes,
    buf: BytesMut,
}

impl FileWatcher<OsFileSystem> {
    /// Create a new `FileWatcher` backed by the OS filesystem and system clock.
    ///
    /// # Errors
    /// Returns an error if opening the file, reading its metadata, or checking
    /// its compression header fails.
    ///
    /// # Panics
    /// Panics if seeking to the initial position in an uncompressed file fails.
    pub async fn new(
        path: PathBuf,
        read_from: ReadFrom,
        ignore_before: Option<DateTime<Utc>>,
        max_line_bytes: usize,
        line_delimiter: Bytes,
    ) -> Result<Self, std::io::Error> {
        let clock = SystemClock;
        Self::new_with_fs(
            path,
            read_from,
            ignore_before,
            max_line_bytes,
            line_delimiter,
            OsFileSystem,
            clock.now(),
            clock.now_utc(),
        )
        .await
    }

    #[cfg(test)]
    pub(super) async fn read_line(&mut self) -> io::Result<RawLineResult> {
        self.read_line_at(SystemClock.now()).await
    }

    #[cfg(test)]
    pub fn should_read(&self) -> bool {
        self.should_read_at(SystemClock.now())
    }
}

impl<FS: FileSystem> FileWatcher<FS> {
    /// Create a new `FileWatcher` using the provided [`FileSystem`] and clock instants.
    ///
    /// # Errors
    /// Returns an error if opening the file, reading its metadata, or checking
    /// its compression header fails.
    ///
    /// # Panics
    /// Panics if seeking to the initial position in an uncompressed file fails.
    #[allow(clippy::too_many_arguments)]
    pub async fn new_with_fs(
        path: PathBuf,
        read_from: ReadFrom,
        ignore_before: Option<DateTime<Utc>>,
        max_line_bytes: usize,
        line_delimiter: Bytes,
        fs: FS,
        now: MonotonicInstant,
        now_utc: DateTime<Utc>,
    ) -> Result<Self, std::io::Error> {
        let f = fs.open(&path).await?;
        let identity = f.identity().await?;
        let (devno, ino) = (identity.dev, identity.ino);
        let metadata = f.metadata().await?;

        let mut reader = BufReader::new(f);

        let too_old = if let (Some(ignore_before), Some(modified_time)) =
            (ignore_before, metadata.modified)
        {
            modified_time < ignore_before
        } else {
            false
        };

        let gzipped = is_gzipped(&mut reader).await?;

        // Determine the actual position at which we should start reading.
        // For non-gzipped files, we use FileReader::Plain which allows metadata access.
        // For gzipped files, position tracking is not meaningful.
        let (reader, file_position): (FileReader<FS::File>, FilePosition) =
            match (gzipped, too_old, read_from) {
                (true, true, _) => {
                    debug!(
                        message = "Not reading gzipped file older than `ignore_older`.",
                        ?path,
                    );
                    (FileReader::Null(io::Cursor::new(Vec::new())), 0)
                }
                (true, _, ReadFrom::Checkpoint(file_position)) => {
                    debug!(
                        message = "Not re-reading gzipped file with existing stored offset.",
                        ?path,
                        %file_position
                    );
                    (FileReader::Null(io::Cursor::new(Vec::new())), file_position)
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
                    (FileReader::Null(io::Cursor::new(Vec::new())), 0)
                }
                (true, false, ReadFrom::Beginning) => (
                    FileReader::Gzipped(Box::new(BufReader::new(gzip_multiple_decoder(reader)))),
                    0,
                ),
                (false, false, ReadFrom::Checkpoint(file_position)) => {
                    let pos = reader.seek(SeekFrom::Start(file_position)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
                (false, false, ReadFrom::Beginning) => {
                    let pos = reader.seek(SeekFrom::Start(0)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
                (false, true, _) | (false, false, ReadFrom::End) => {
                    let pos = reader.seek(SeekFrom::End(0)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
            };

        let ts = metadata
            .modified
            .and_then(|mtime| now_utc.signed_duration_since(mtime).to_std().ok())
            .and_then(|diff| now.checked_sub(diff))
            .unwrap_or(now);

        Ok(FileWatcher {
            path,
            fs,
            findable: true,
            reader,
            file_position,
            devno,
            inode: ino,
            is_dead: false,
            reached_eof: false,
            last_read_attempt: ts,
            last_read_success: ts,
            read_retry_delay: EOF_READ_BACKOFF_MIN,
            last_seen: ts,
            max_line_bytes,
            line_delimiter,
            buf: BytesMut::new(),
        })
    }

    /// Update the path being watched.
    ///
    /// If the file at the new path has a different inode, this indicates the file
    /// was replaced (not just renamed). In this case, returns `FileUnwatchInfo`
    /// containing metrics about the old file so the caller can emit appropriate events.
    ///
    /// # Errors
    /// Returns an error if opening the replacement file, reading its metadata
    /// or compression header, or restoring the file position fails.
    pub async fn update_path(&mut self, path: PathBuf) -> io::Result<Option<FileUnwatchInfo>> {
        let new_file = self.fs.open(&path).await?;

        let identity = new_file.identity().await?;
        let unwatch_info = if (identity.dev, identity.ino) == (self.devno, self.inode) {
            None
        } else {
            // Capture metrics from the old file before switching
            let old_info = self.get_unwatch_info().await;

            let mut reader = BufReader::new(new_file);
            let gzipped = is_gzipped(&mut reader).await?;
            let new_reader = if gzipped {
                if self.file_position != 0 {
                    FileReader::Null(io::Cursor::new(Vec::new()))
                } else {
                    FileReader::Gzipped(Box::new(BufReader::new(gzip_multiple_decoder(reader))))
                }
            } else {
                reader.seek(io::SeekFrom::Start(self.file_position)).await?;
                FileReader::Plain(reader)
            };
            self.reader = new_reader;

            self.devno = identity.dev;
            self.inode = identity.ino;

            Some(old_info)
        };

        self.reached_eof = false;
        self.read_retry_delay = EOF_READ_BACKOFF_MIN;
        self.path = path;
        Ok(unwatch_info)
    }

    pub fn set_file_findable_at(&mut self, f: bool, now: MonotonicInstant) {
        self.findable = f;
        if f {
            self.last_seen = now;
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

    /// Returns the number of bytes that were not read.
    /// Uses the current file size from the underlying File (works even after
    /// file deletion since the fd remains valid).
    /// Returns None for metadata errors, gzipped files, or skipped readers.
    /// For gzip, `file_position` tracks decompressed bytes while the on-disk size is
    /// compressed, so the subtraction would be meaningless, even after reaching EOF.
    /// Some(0) means the size is known and no bytes remain unread at measurement time.
    pub async fn get_bytes_unread(&self) -> Option<u64> {
        self.reader
            .file_size()
            .await
            .map(|current_size| current_size.saturating_sub(self.file_position))
    }

    /// Returns information about this file for metric emission when unwatching.
    /// This provides a consistent interface for all unwatch scenarios.
    pub async fn get_unwatch_info(&self) -> FileUnwatchInfo {
        FileUnwatchInfo {
            path: self.path.clone(),
            bytes_unread: self.get_bytes_unread().await,
            reached_eof: self.reached_eof,
        }
    }

    /// Read a single line from the underlying file at the given monotonic timestamp.
    pub(super) async fn read_line_at(
        &mut self,
        now: MonotonicInstant,
    ) -> io::Result<RawLineResult> {
        self.track_read_attempt(now);

        let reader = &mut self.reader;
        let file_position = &mut self.file_position;
        let initial_position = *file_position;
        match read_until_with_max_size(
            reader,
            file_position,
            self.line_delimiter.as_ref(),
            &mut self.buf,
            self.max_line_bytes,
        )
        .await
        {
            Ok(ReadResult {
                successfully_read: Some(_),
                discarded_for_size_and_truncated,
            }) => {
                self.track_read_success(now);
                Ok(RawLineResult {
                    raw_line: Some(RawLine {
                        offset: initial_position,
                        bytes: self.buf.split().freeze(),
                    }),
                    discarded_for_size_and_truncated,
                })
            }
            Ok(ReadResult {
                successfully_read: None,
                discarded_for_size_and_truncated,
            }) => {
                if self.file_findable() {
                    self.track_read_eof();
                    Ok(RawLineResult {
                        raw_line: None,
                        discarded_for_size_and_truncated,
                    })
                } else {
                    self.set_dead();
                    // File has been deleted, so return what we have in the buffer, even though it
                    // didn't end with a newline. This is not a perfect signal for when we should
                    // give up waiting for a newline, but it's decent.
                    let buf = self.buf.split().freeze();
                    if buf.is_empty() {
                        // EOF
                        self.reached_eof = true;
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
    fn track_read_attempt(&mut self, now: MonotonicInstant) {
        self.last_read_attempt = now;
    }

    #[inline]
    fn track_read_success(&mut self, now: MonotonicInstant) {
        self.reached_eof = false;
        self.read_retry_delay = EOF_READ_BACKOFF_MIN;
        self.last_read_success = now;
    }

    #[inline]
    fn track_read_eof(&mut self) {
        self.read_retry_delay = if self.reached_eof {
            std::cmp::min(
                self.read_retry_delay.saturating_mul(2),
                EOF_READ_BACKOFF_MAX,
            )
        } else {
            EOF_READ_BACKOFF_MIN
        };
        self.reached_eof = true;
    }

    #[inline]
    pub fn last_read_success(&self) -> MonotonicInstant {
        self.last_read_success
    }

    #[inline]
    pub fn should_read_at(&self, now: MonotonicInstant) -> bool {
        if self.reached_eof
            && now.saturating_duration_since(self.last_read_attempt) < self.read_retry_delay
        {
            return false;
        }

        now.saturating_duration_since(self.last_read_success) < Duration::from_secs(10)
            || now.saturating_duration_since(self.last_read_attempt) > Duration::from_secs(10)
    }

    #[inline]
    pub fn last_seen(&self) -> MonotonicInstant {
        self.last_seen
    }

    #[inline]
    pub fn reached_eof(&self) -> bool {
        self.reached_eof
    }
}

async fn is_gzipped<R: AsyncRead + Unpin>(r: &mut BufReader<R>) -> io::Result<bool> {
    let header_bytes = r.fill_buf().await?;
    // WARN: The paired `BufReader::consume` is not called intentionally. If we
    // do we'll chop a decent part of the potential gzip stream off.
    Ok(header_bytes.starts_with(GZIP_MAGIC))
}
