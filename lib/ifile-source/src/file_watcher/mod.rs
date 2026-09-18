use std::{
    io::{self, SeekFrom},
    path::PathBuf,
    time::Instant,
};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use tokio::{
    fs::{self, File},
    io::{AsyncBufRead, AsyncBufReadExt, AsyncSeekExt, BufReader},
};
use tracing::debug;
use vector_common::compression::gzip_multiple_decoder;
use vector_common::constants::GZIP_MAGIC;

use crate::{FilePosition, ReadFrom};
use file_source_common::PortableFileExt;
use file_source_common::{
    buffer::{read_until_with_max_size, ReadResult},
    AsyncFileInfo,
};

/// The `RawLine` struct is a thin wrapper around the bytes that have been read
/// in order to retain the context of where in the file they have been read from.
///
/// The offset field contains the byte offset of the beginning of the line within
/// the file that it was read from.
#[derive(Debug)]
pub struct RawLine {
    pub offset: FilePosition,
    pub bytes: Bytes,
}

/// The `FileWatcher` struct defines the state machine which reads
/// from a file path, transparently handling file rollovers as is common for logs.
///
/// Plain files retain their handle so writes remain readable after a rename.
/// Compressed files retain their decoder and handle until EOF so decompression
/// can continue across read batches.
///
/// The `FileWatcher` is expected to live for the lifetime of the file
/// path. `FileServer` is responsible for clearing away `FileWatchers` which no
/// longer exist.
pub struct FileWatcher {
    pub path: PathBuf,
    findable: bool,
    file_position: FilePosition,
    devno: u64,
    inode: u64,
    is_dead: bool,
    reached_eof: bool,
    last_read_success: Instant,
    last_seen: Instant,
    max_line_bytes: usize,
    line_delimiter: Bytes,
    buf: BytesMut,
    reader: FileReader,
}

enum FileReader {
    Plain(BufReader<File>),
    Gzip(Box<dyn AsyncBufRead + Send + Unpin>),
    Empty,
}

impl FileWatcher {
    /// Create a new `FileWatcher` without reading its contents.
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
    ) -> Result<FileWatcher, io::Error> {
        let f = fs::File::open(&path).await?;
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

        // Determine the actual position at which we should start reading
        let (reader, file_position): (FileReader, FilePosition) =
            match (gzipped, too_old, read_from) {
                (true, true, _) => {
                    debug!(
                        message = "Not reading gzipped file older than `ignore_older`.",
                        ?path,
                    );
                    (FileReader::Empty, 0)
                }
                (true, _, ReadFrom::Checkpoint(file_position)) => {
                    debug!(
                        message = "Not re-reading gzipped file with existing stored offset.",
                        ?path,
                        %file_position
                    );
                    (FileReader::Empty, file_position)
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
                    (FileReader::Empty, 0)
                }
                (true, false, ReadFrom::Beginning) => (
                    FileReader::Gzip(Box::new(BufReader::new(gzip_multiple_decoder(reader)))),
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

        Ok(FileWatcher {
            path: path.clone(),
            findable: true,
            file_position,
            devno,
            inode: ino,
            is_dead: false,
            reached_eof: false,
            last_read_success: ts,
            last_seen: ts,
            max_line_bytes,
            line_delimiter,
            buf: BytesMut::new(),
            reader,
        })
    }

    pub async fn update_path(&mut self, path: PathBuf) -> io::Result<()> {
        let file_handle = File::open(&path).await?;
        let file_info = file_handle.file_info().await?;
        if (file_info.portable_dev(), file_info.portable_ino()) != (self.devno, self.inode) {
            if matches!(self.reader, FileReader::Plain(_)) {
                let mut reader = BufReader::new(file_handle);
                reader.seek(SeekFrom::Start(self.file_position)).await?;
                self.reader = FileReader::Plain(reader);
            }
            self.devno = file_info.portable_dev();
            self.inode = file_info.portable_ino();
        }
        self.path = path;
        Ok(())
    }

    pub(super) fn same_file(&self, other: &Self) -> bool {
        (self.devno, self.inode) == (other.devno, other.inode)
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

    pub fn reached_eof(&self) -> bool {
        self.reached_eof
    }

    /// Read a single line from the underlying file
    ///
    /// This function will attempt to read a new line from its file, blocking,
    /// up to some maximum but unspecified amount of time.
    pub(super) async fn read_line(&mut self) -> io::Result<Option<RawLine>> {
        if self.is_dead {
            return Ok(None);
        }

        let reader: &mut (dyn AsyncBufRead + Send + Unpin) = match &mut self.reader {
            FileReader::Plain(reader) => {
                // copy-truncate preserves the inode but resets the file's contents.
                if reader.get_ref().metadata().await?.len() < self.file_position {
                    reader.seek(SeekFrom::Start(0)).await?;
                    self.file_position = 0;
                    self.buf.clear();
                }
                reader
            }
            FileReader::Gzip(reader) => reader.as_mut(),
            FileReader::Empty => {
                self.reached_eof = true;
                return Ok(None);
            }
        };
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
                successfully_read: Some(_), // TODO check if discarded_for_size_and_truncated is
                                            // empty
                ..
            }) => {
                self.reached_eof = false;
                self.track_read_success();
                let bytes = self.buf.split().freeze();

                debug!(
                    "read_line {}",
                    String::from_utf8_lossy(bytes::Buf::chunk(&bytes))
                );
                // Return all lines, including empty ones
                Ok(Some(RawLine {
                    offset: initial_position,
                    bytes,
                }))
            }
            Ok(ReadResult {
                successfully_read: None,
                ..
            }) => {
                if matches!(self.reader, FileReader::Gzip(_)) {
                    self.reader = FileReader::Empty;
                }
                // A renamed file can still receive writes through an open handle.
                // FileServer retires it after rotate_wait, not at the first EOF.
                self.reached_eof = true;
                Ok(None)
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
    fn track_read_success(&mut self) {
        self.last_read_success = Instant::now();
    }

    #[inline]
    pub fn last_read_success(&self) -> Instant {
        self.last_read_success
    }

    #[inline]
    pub fn last_seen(&self) -> Instant {
        self.last_seen
    }
}

async fn is_gzipped(r: &mut BufReader<File>) -> io::Result<bool> {
    let header_bytes = r.fill_buf().await?;
    // WARN: The paired `BufReader::consume` is not called intentionally. If we
    // do we'll chop a decent part of the potential gzip stream off.
    Ok(header_bytes.starts_with(GZIP_MAGIC))
}
