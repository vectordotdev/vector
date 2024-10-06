use std::{
    fs::{self, File},
    io::{self, BufRead, Seek},
    path::PathBuf,
    time::{Duration, Instant},
};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use flate2::bufread::MultiGzDecoder;
use tracing::debug;

use crate::{
    buffer::read_until_with_max_size, metadata_ext::PortableFileExt, FilePosition, ReadFrom,
};
#[cfg(test)]
mod tests;

/// The `RawLine` struct is a thin wrapper around the bytes that have been read
/// in order to retain the context of where in the file they have been read from.
///
/// The offset field contains the byte offset of the beginning of the line within
/// the file that it was read from.
#[derive(Debug)]
pub(super) struct RawLine {
    pub offset: u64,
    pub bytes: Bytes,
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
    reader: Box<dyn BufRead>,
    file_position: FilePosition,
    last_line_offset: u64,
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
}

impl FileWatcher {
    /// Create a new `FileWatcher`
    ///
    /// The input path will be used by `FileWatcher` to prime its state
    /// machine. A `FileWatcher` tracks _only one_ file. This function returns
    /// None if the path does not exist or is not readable by the current process.
    pub fn new(
        path: PathBuf,
        read_from: ReadFrom,
        ignore_before: Option<DateTime<Utc>>,
        max_line_bytes: usize,
        line_delimiter: Bytes,
    ) -> Result<FileWatcher, io::Error> {
        let f = fs::File::open(&path)?;
        let (devno, ino) = (f.portable_dev()?, f.portable_ino()?);
        let metadata = f.metadata()?;
        let mut reader = io::BufReader::new(f);

        let too_old = if let (Some(ignore_before), Ok(modified_time)) = (
            ignore_before,
            metadata.modified().map(DateTime::<Utc>::from),
        ) {
            modified_time < ignore_before
        } else {
            false
        };

        let gzipped = is_gzipped(&mut reader)?;

        // Determine the actual position at which we should start reading
        let (reader, file_position): (Box<dyn BufRead>, FilePosition) =
            match (gzipped, too_old, read_from) {
                (true, true, _) => {
                    debug!(
                        message = "Not reading gzipped file older than `ignore_older`.",
                        ?path,
                    );
                    (Box::new(null_reader()), 0)
                }
                (true, _, ReadFrom::Checkpoint(file_position)) => {
                    debug!(
                        message = "Not re-reading gzipped file with existing stored offset.",
                        ?path,
                        %file_position
                    );
                    (Box::new(null_reader()), file_position)
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
                    (Box::new(null_reader()), 0)
                }
                (true, false, ReadFrom::Beginning) => {
                    (Box::new(io::BufReader::new(MultiGzDecoder::new(reader))), 0)
                }
                (false, true, _) => {
                    let pos = reader.seek(io::SeekFrom::End(0)).unwrap();
                    (Box::new(reader), pos)
                }
                (false, false, ReadFrom::Checkpoint(file_position)) => {
                    let pos = reader.seek(io::SeekFrom::Start(file_position)).unwrap();
                    (Box::new(reader), pos)
                }
                (false, false, ReadFrom::Beginning) => {
                    let pos = reader.seek(io::SeekFrom::Start(0)).unwrap();
                    (Box::new(reader), pos)
                }
                (false, false, ReadFrom::End) => {
                    let pos = reader.seek(io::SeekFrom::End(0)).unwrap();
                    (Box::new(reader), pos)
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
            reader,
            file_position,
            last_line_offset: file_position,
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
        })
    }

    pub fn update_path(&mut self, path: PathBuf) -> io::Result<()> {
        let file_handle = File::open(&path)?;
        if (file_handle.portable_dev()?, file_handle.portable_ino()?) != (self.devno, self.inode) {
            let mut reader = io::BufReader::new(fs::File::open(&path)?);
            let gzipped = is_gzipped(&mut reader)?;
            let new_reader: Box<dyn BufRead> = if gzipped {
                if self.file_position != 0 {
                    Box::new(null_reader())
                } else {
                    Box::new(io::BufReader::new(MultiGzDecoder::new(reader)))
                }
            } else {
                reader.seek(io::SeekFrom::Start(self.file_position))?;
                Box::new(reader)
            };
            self.reader = new_reader;
            self.devno = file_handle.portable_dev()?;
            self.inode = file_handle.portable_ino()?;
        }
        self.path = path;
        Ok(())
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

    /// Read a single line from the underlying file
    ///
    /// This function will attempt to read a new line from its file, blocking,
    /// up to some maximum but unspecified amount of time. `read_line` will open
    /// a new file handler as needed, transparently to the caller.
    pub(super) fn read_line(&mut self) -> io::Result<Option<RawLine>> {
        self.track_read_attempt();
        self.track_last_line_offset();

        let line_offset = self.last_line_offset;
        let reader = &mut self.reader;
        let file_position = &mut self.file_position;

        match read_until_with_max_size(
            reader,
            file_position,
            self.line_delimiter.as_ref(),
            &mut self.buf,
            self.max_line_bytes,
        ) {
            Ok(Some(_)) => {
                self.track_read_success();
                Ok(Some(RawLine {
                    offset: line_offset,
                    bytes: self.buf.split().freeze(),
                }))
            }
            Ok(None) => {
                if !self.file_findable() {
                    self.set_dead();
                    // File has been deleted, so return what we have in the buffer, even though it
                    // didn't end with a newline. This is not a perfect signal for when we should
                    // give up waiting for a newline, but it's decent.
                    let buf = self.buf.split().freeze();
                    if buf.is_empty() {
                        // EOF
                        self.reached_eof = true;
                        Ok(None)
                    } else {
                        Ok(Some(RawLine {
                            offset: line_offset,
                            bytes: buf,
                        }))
                    }
                } else {
                    self.reached_eof = true;
                    Ok(None)
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
    fn track_last_line_offset(&mut self) {
        if self.buf.is_empty() {
            self.last_line_offset = self.file_position
        }
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

fn is_gzipped(r: &mut io::BufReader<fs::File>) -> io::Result<bool> {
    let header_bytes = r.fill_buf()?;
    // WARN: The paired `BufReader::consume` is not called intentionally. If we
    // do we'll chop a decent part of the potential gzip stream off.
    Ok(header_bytes.starts_with(&[0x1f, 0x8b]))
}

fn null_reader() -> impl BufRead {
    io::Cursor::new(Vec::new())
}
