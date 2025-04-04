use std::{
    fs::{self, File},
    io::{self, BufRead, Seek},
    path::PathBuf,
    time::{Duration, Instant},
};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use flate2::bufread::MultiGzDecoder;
use tracing::{debug, trace};
use vector_common::constants::GZIP_MAGIC;

use crate::{
    buffer::read_until_with_max_size, metadata_ext::PortableFileExt, FilePosition, ReadFrom,
};
#[cfg(test)]
mod tests;
mod notify_watcher;

use notify_watcher::NotifyWatcher;

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

/// Represents the state of the file watcher
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherState {
    /// Actively watching the file with an open file handle
    Active,
    /// Passively watching the file using filesystem notifications
    Passive,
}

/// The `FileWatcher` struct defines the state machine which reads
/// from a file path, transparently updating the underlying file descriptor when
/// the file has been rolled over, as is common for logs.
///
/// The `FileWatcher` can operate in two modes:
/// 1. Active mode: Uses polling with an open file handle (original behavior)
/// 2. Passive mode: Uses filesystem notifications without holding an open file handle
///
/// The `FileWatcher` is expected to live for the lifetime of the file
/// path. `FileServer` is responsible for clearing away `FileWatchers` which no
/// longer exist.
pub struct FileWatcher {
    pub path: PathBuf,
    findable: bool,
    reader: Option<Box<dyn BufRead>>,
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
    /// Current state of the watcher
    state: WatcherState,
    /// Notify-based watcher for passive mode
    notify_watcher: Option<NotifyWatcher>,
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

        // Create a notify watcher for passive mode
        let notify_watcher = NotifyWatcher::new().ok();

        Ok(FileWatcher {
            path,
            findable: true,
            reader: Some(reader),
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
            state: WatcherState::Active,
            notify_watcher,
        })
    }

    pub fn update_path(&mut self, path: PathBuf) -> io::Result<()> {
        let file_handle = File::open(&path)?;
        if (file_handle.portable_dev()?, file_handle.portable_ino()?) != (self.devno, self.inode) {
            // If we're in active mode, update the reader
            if self.state == WatcherState::Active {
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
                self.reader = Some(new_reader);
            } else if let Some(ref mut notify_watcher) = self.notify_watcher {
                // Update the notify watcher with the new path
                if let Err(e) = notify_watcher.watch_file(path.clone(), self.file_position) {
                    debug!(message = "Failed to update notify watcher", error = ?e);
                }
            }
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

    pub fn reached_eof(&self) -> bool {
        self.reached_eof
    }

    /// Read a single line from the underlying file
    ///
    /// This function will attempt to read a new line from its file, blocking,
    /// up to some maximum but unspecified amount of time. `read_line` will open
    /// a new file handler as needed, transparently to the caller.
    pub(super) fn read_line(&mut self) -> io::Result<Option<RawLine>> {
        self.track_read_attempt();

        if self.is_dead {
            return Ok(None);
        }

        // If we're in passive mode, check for events and potentially switch to active mode
        if self.state == WatcherState::Passive {
            if let Some(ref mut notify_watcher) = self.notify_watcher {
                let events = notify_watcher.check_events();
                for (path, kind) in events {
                    if path == self.path {
                        trace!(message = "Detected file event, activating watcher", ?path, ?kind);
                        self.activate()?;
                        // Note: The FileServer will handle emitting the event when it processes the file
                        break;
                    }
                }
            }

            // If still in passive mode, return None (no data available)
            if self.state == WatcherState::Passive {
                return Ok(None);
            }
        }

        let reader = match &mut self.reader {
            Some(r) => r,
            None => return Ok(None),
        };
        let file_position = &mut self.file_position;
        let initial_position = *file_position;
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
                    offset: initial_position,
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
                            offset: initial_position,
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

    /// Activate the watcher (switch from passive to active mode)
    ///
    /// This opens a file handle and starts reading from the file.
    pub fn activate(&mut self) -> io::Result<()> {
        if self.state == WatcherState::Active {
            return Ok(());
        }

        debug!(message = "Activating file watcher", ?self.path);

        // Open the file and create a reader
        let f = fs::File::open(&self.path)?;
        let mut reader = io::BufReader::new(f);

        // Seek to the last known position
        reader.seek(io::SeekFrom::Start(self.file_position))?;

        self.reader = Some(Box::new(reader));
        self.state = WatcherState::Active;
        self.last_read_attempt = Instant::now();
        self.reached_eof = false;

        Ok(())
    }

    /// Deactivate the watcher (switch from active to passive mode)
    ///
    /// This closes the file handle but keeps tracking the file position
    /// and watches for changes using filesystem notifications.
    pub fn deactivate(&mut self) -> io::Result<()> {
        if self.state == WatcherState::Passive {
            return Ok(());
        }

        debug!(message = "Deactivating file watcher", ?self.path, position = %self.file_position);

        // Initialize the notify watcher if it doesn't exist
        if self.notify_watcher.is_none() {
            self.notify_watcher = NotifyWatcher::new().ok();
        }

        // Add the file to the notify watcher
        if let Some(ref mut notify_watcher) = self.notify_watcher {
            if let Err(e) = notify_watcher.watch_file(self.path.clone(), self.file_position) {
                debug!(message = "Failed to add file to notify watcher", error = ?e);
            }
        }

        // Drop the reader to close the file handle
        self.reader = None;
        self.state = WatcherState::Passive;

        Ok(())
    }

    /// Get the current state of the watcher
    pub fn state(&self) -> WatcherState {
        self.state
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


}

fn is_gzipped(r: &mut io::BufReader<fs::File>) -> io::Result<bool> {
    let header_bytes = r.fill_buf()?;
    // WARN: The paired `BufReader::consume` is not called intentionally. If we
    // do we'll chop a decent part of the potential gzip stream off.
    Ok(header_bytes.starts_with(GZIP_MAGIC))
}

fn null_reader() -> impl BufRead {
    io::Cursor::new(Vec::new())
}
