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
use tokio::{
    fs::File,
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncSeekExt, BufReader, ReadBuf},
    time::Instant,
};
use tracing::debug;
use vector_common::constants::GZIP_MAGIC;

use file_source_common::{
    AsyncFileInfo, FilePosition, PortableFileExt, ReadFrom,
    buffer::{ReadResult, read_until_with_max_size},
};

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
/// BufReader through `get_ref()`. This is critical for accurately tracking
/// `bytes_unread` even after file deletion (the fd remains valid).
enum FileReader {
    /// Plain file reader - we can access the File via get_ref() for metadata
    Plain(BufReader<File>),
    /// Gzipped file reader - no meaningful file position tracking
    Gzipped(BufReader<GzipDecoder<BufReader<File>>>),
    /// Null reader for skipped files
    Null(io::Cursor<Vec<u8>>),
}

impl FileReader {
    /// Get the current file size by accessing the underlying File.
    /// Returns None for gzipped or null readers where file size isn't meaningful.
    /// This works even after the file has been deleted (on Unix) since the fd is still open.
    async fn file_size(&self) -> Option<u64> {
        match self {
            FileReader::Plain(reader) => reader.get_ref().metadata().await.ok().map(|m| m.len()),
            FileReader::Gzipped(_) | FileReader::Null(_) => None,
        }
    }
}

impl AsyncRead for FileReader {
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

impl AsyncBufRead for FileReader {
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
    /// Number of bytes that were not read from the file
    pub bytes_unread: u64,
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
pub struct FileWatcher {
    pub path: PathBuf,
    findable: bool,
    reader: FileReader,
    file_position: FilePosition,
    devno: u64,
    inode: u64,
    is_dead: bool,
    reached_eof: bool,
    last_read_attempt: Instant,
    last_read_success: Instant,
    last_seen: Instant,
    max_line_bytes: usize,
    line_delimiter: Bytes,
    buf: BytesMut,
    /// The file size when the watcher was created. Used as fallback for
    /// bytes unread calculation when the reader doesn't support file_size().
    initial_file_size: u64,
}

impl FileWatcher {
    /// Create a new `FileWatcher`
    ///
    /// The input path will be used by `FileWatcher` to prime its state
    /// machine. A `FileWatcher` tracks _only one_ file. This function returns
    /// None if the path does not exist or is not readable by the current process.
    pub async fn new(
        path: PathBuf,
        read_from: ReadFrom,
        ignore_before: Option<DateTime<Utc>>,
        max_line_bytes: usize,
        line_delimiter: Bytes,
    ) -> Result<FileWatcher, std::io::Error> {
        let f = File::open(&path).await?;
        let file_info = f.file_info().await?;
        let (devno, ino) = (file_info.portable_dev(), file_info.portable_ino());

        #[cfg(unix)]
        let metadata = file_info;
        #[cfg(windows)]
        let metadata = f.metadata().await?;

        let mut reader = BufReader::new(f);

        let too_old = if let (Some(ignore_before), Ok(modified_time)) = (
            ignore_before,
            metadata.modified().map(DateTime::<Utc>::from),
        ) {
            modified_time < ignore_before
        } else {
            false
        };

        let gzipped = is_gzipped(&mut reader).await?;

        // Determine the actual position at which we should start reading.
        // For non-gzipped files, we use FileReader::Plain which allows metadata access.
        // For gzipped files, position tracking is not meaningful.
        let (reader, file_position): (FileReader, FilePosition) =
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
                    FileReader::Gzipped(BufReader::new(GzipDecoder::new(reader))),
                    0,
                ),
                (false, true, _) => {
                    let pos = reader.seek(SeekFrom::End(0)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
                (false, false, ReadFrom::Checkpoint(file_position)) => {
                    let pos = reader.seek(SeekFrom::Start(file_position)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
                (false, false, ReadFrom::Beginning) => {
                    let pos = reader.seek(SeekFrom::Start(0)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
                (false, false, ReadFrom::End) => {
                    let pos = reader.seek(SeekFrom::End(0)).await.unwrap();
                    (FileReader::Plain(reader), pos)
                }
            };

        let ts = metadata
            .modified()
            .ok()
            .and_then(|mtime| mtime.elapsed().ok())
            .and_then(|diff| Instant::now().checked_sub(diff))
            .unwrap_or_else(Instant::now);

        let initial_file_size = metadata.len();

        Ok(FileWatcher {
            path,
            findable: true,
            reader,
            file_position,
            devno,
            inode: ino,
            is_dead: false,
            reached_eof: false,
            last_read_attempt: ts,
            last_read_success: ts,
            last_seen: ts,
            max_line_bytes,
            line_delimiter,
            buf: BytesMut::new(),
            initial_file_size,
        })
    }

    /// Update the path being watched.
    ///
    /// If the file at the new path has a different inode, this indicates the file
    /// was replaced (not just renamed). In this case, returns `FileUnwatchInfo`
    /// containing metrics about the old file so the caller can emit appropriate events.
    pub async fn update_path(&mut self, path: PathBuf) -> io::Result<Option<FileUnwatchInfo>> {
        let new_file = File::open(&path).await?;

        let file_info = new_file.file_info().await?;
        let unwatch_info =
            if (file_info.portable_dev(), file_info.portable_ino()) != (self.devno, self.inode) {
                // Capture metrics from the old file before switching
                let old_info = self.get_unwatch_info().await;

                let mut reader = BufReader::new(new_file);
                let gzipped = is_gzipped(&mut reader).await?;
                let new_reader = if gzipped {
                    if self.file_position != 0 {
                        FileReader::Null(io::Cursor::new(Vec::new()))
                    } else {
                        FileReader::Gzipped(BufReader::new(GzipDecoder::new(reader)))
                    }
                } else {
                    reader.seek(io::SeekFrom::Start(self.file_position)).await?;
                    FileReader::Plain(reader)
                };
                self.reader = new_reader;

                self.devno = file_info.portable_dev();
                self.inode = file_info.portable_ino();
                // Reset initial_file_size for the new file
                self.initial_file_size = file_info.len();

                Some(old_info)
            } else {
                None
            };

        self.path = path;
        Ok(unwatch_info)
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

    /// Returns the number of bytes that were not read.
    /// Uses the current file size from the underlying File (works even after
    /// file deletion since the fd remains valid), falling back to initial_file_size.
    /// When the file reaches EOF, this will be 0. When the file is unwatched before EOF,
    /// this represents the bytes that were never read.
    pub async fn get_bytes_unread(&self) -> u64 {
        let current_size = self
            .reader
            .file_size()
            .await
            .unwrap_or(self.initial_file_size);

        current_size.saturating_sub(self.file_position)
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

    /// Read a single line from the underlying file
    ///
    /// This function will attempt to read a new line from its file, blocking,
    /// up to some maximum but unspecified amount of time. `read_line` will open
    /// a new file handler as needed, transparently to the caller.
    pub(super) async fn read_line(&mut self) -> io::Result<RawLineResult> {
        self.track_read_attempt();

        let reader = &mut self.reader;
        let file_position = &mut self.file_position;
        let initial_position = *file_position;
        match read_until_with_max_size(
            Box::pin(reader),
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
                self.track_read_success();
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
                if !self.file_findable() {
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
                } else {
                    self.reached_eof = true;
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
        self.last_read_attempt = Instant::now();
    }

    #[inline]
    fn track_read_success(&mut self) {
        self.last_read_success = Instant::now();
    }

    #[inline]
    pub fn last_read_success(&self) -> Instant {
        self.last_read_success
    }

    #[inline]
    pub fn should_read(&self) -> bool {
        self.last_read_success.elapsed() < Duration::from_secs(10)
            || self.last_read_attempt.elapsed() > Duration::from_secs(10)
    }

    #[inline]
    pub fn last_seen(&self) -> Instant {
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
