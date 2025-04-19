use std::{
    fs::{self, File},
    io::{self, BufRead, Seek},
    path::PathBuf,
    time::{Duration, Instant},
};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use flate2::bufread::MultiGzDecoder;
use tracing::{debug, trace, warn};
use vector_common::constants::GZIP_MAGIC;

use crate::{
    buffer::read_until_with_max_size, metadata_ext::PortableFileExt, FilePosition, ReadFrom,
};
mod notify_watcher;
#[cfg(test)]
mod tests;

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
///
/// Note: Previously, we had Active and Passive states, but now we only use
/// notification-based watching for all files, so we only need one state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherState {
    /// Watching the file using filesystem notifications
    Notify,
}

/// The `FileWatcher` struct defines the state machine which reads
/// from a file path, transparently handling file rollovers as is common for logs.
///
/// The `FileWatcher` uses filesystem notifications exclusively for all files,
/// without keeping any files open. Files are only opened when needed for reading,
/// then closed immediately.
///
/// The `FileWatcher` is expected to live for the lifetime of the file
/// path. `FileServer` is responsible for clearing away `FileWatchers` which no
/// longer exist.
pub struct FileWatcher {
    pub path: PathBuf,
    findable: bool,
    #[allow(dead_code)]
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
    /// Current state of the watcher (always Notify)
    #[allow(dead_code)]
    state: WatcherState,
    /// Notify-based watcher for all files
    notify_watcher: NotifyWatcher,
    /// Buffer of lines read at startup
    startup_lines: Vec<RawLine>,
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
        let (_reader, file_position): (Box<dyn BufRead>, FilePosition) =
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

        // On startup, we need to read any content that was added while Vector was stopped
        // We'll do this by immediately reading the file once to get any new content
        // After that, we'll rely on notifications for further changes

        // Create a notify watcher for all files
        let notify_watcher = match NotifyWatcher::new() {
            Ok(mut watcher) => {
                // Start watching this file immediately
                if let Err(e) = watcher.watch_file(path.clone(), file_position) {
                    debug!(message = "Failed to set up file watcher", path = ?path, error = ?e);
                }
                watcher
            },
            Err(e) => {
                warn!(message = "Failed to create notify watcher", error = ?e);
                // Create a dummy watcher that doesn't do anything
                NotifyWatcher::dummy()
            }
        };

        // We don't need to keep the file open permanently, but we'll read it once
        // to get any content that was added while Vector was stopped
        // This is especially important for files with checkpoints

        // Create the FileWatcher instance
        let mut fw = FileWatcher {
            path: path.clone(),
            findable: true,
            reader: None, // No reader is kept open
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
            state: WatcherState::Notify, // Always in Notify state
            notify_watcher,
            startup_lines: Vec::new(),
        };

        // Read all available lines at startup to get any content that was added while Vector was stopped
        // This is especially important for files with checkpoints
        // We'll ignore any errors since the file might not be readable yet
        debug!(message = "Reading initial content from file at startup", ?path, position = file_position);

        // Keep reading until we reach EOF or encounter an error
        loop {
            match fw.read_line() {
                Ok(Some(line)) => {
                    // Successfully read a line, store it and continue reading
                    trace!(message = "Read a line from file at startup", ?path, new_position = fw.file_position);
                    fw.startup_lines.push(line);
                },
                Ok(None) => {
                    // Reached EOF
                    debug!(message = "Reached EOF while reading initial content", ?path, final_position = fw.file_position, lines_read = fw.startup_lines.len());
                    break;
                },
                Err(e) => {
                    // Error reading file, log and break
                    debug!(message = "Error reading initial content from file", ?path, error = ?e);
                    break;
                }
            }
        }

        Ok(fw)
    }

    pub fn update_path(&mut self, path: PathBuf) -> io::Result<()> {
        let file_handle = File::open(&path)?;
        if (file_handle.portable_dev()?, file_handle.portable_ino()?) != (self.devno, self.inode) {
            // Update the notify watcher with the new path
            if let Err(e) = self.notify_watcher.watch_file(path.clone(), self.file_position) {
                debug!(message = "Failed to update notify watcher", error = ?e);
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

        // Check for events from the notify watcher, but don't update the watcher here
        // This avoids an infinite loop where reading the file triggers events
        let events = self.notify_watcher.check_events();
        if !events.is_empty() {
            trace!(message = "Checking events for file", count = events.len(), path = ?self.path);

            for (path, kind) in events {
                if path == self.path {
                    debug!(
                        message = "Detected relevant file event",
                        ?path,
                        ?kind
                    );
                    // We don't need to update the watcher here since we're about to read the file
                    // and the position will be updated naturally
                    break;
                }
            }
        }

        // Open the file for reading
        let mut file = match fs::File::open(&self.path) {
            Ok(f) => f,
            Err(e) => {
                if let io::ErrorKind::NotFound = e.kind() {
                    self.set_dead();
                }
                return Err(e);
            }
        };

        // Seek to the current position
        file.seek(io::SeekFrom::Start(self.file_position))?;

        // Create a reader
        let mut reader = io::BufReader::new(file);
        let file_position = &mut self.file_position;
        let initial_position = *file_position;
        match read_until_with_max_size(
            &mut reader,
            file_position,
            self.line_delimiter.as_ref(),
            &mut self.buf,
            self.max_line_bytes,
        ) {
            Ok(Some(_)) => {
                self.track_read_success();
                let bytes = self.buf.split().freeze();

                // Don't return empty lines
                if bytes.is_empty() {
                    trace!(message = "Skipping empty line", ?self.path, position = initial_position);
                    Ok(None)
                } else {
                    Ok(Some(RawLine {
                        offset: initial_position,
                        bytes,
                    }))
                }
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
                        // We already checked that buf is not empty, so we can just return it
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

    /// Update the watcher with the current file position
    ///
    /// This updates the notify watcher with the current file position.
    pub fn update_watcher(&mut self) -> io::Result<()> {
        // Only log at trace level to avoid excessive logging
        trace!(message = "Updating file watcher", ?self.path, position = %self.file_position);

        // Update the notify watcher with the current position
        if let Err(e) = self.notify_watcher.watch_file(self.path.clone(), self.file_position) {
            debug!(message = "Failed to update notify watcher", error = ?e);
        }

        Ok(())
    }

    /// Get the current state of the watcher
    ///
    /// Note: This always returns WatcherState::Notify since we're always using
    /// notification-based watching for all files.
    #[allow(dead_code)]
    pub fn state(&self) -> WatcherState {
        WatcherState::Notify
    }

    /// Get and clear the startup lines
    ///
    /// This returns all lines read at startup and clears the buffer.
    pub fn take_startup_lines(&mut self) -> Vec<RawLine> {
        std::mem::take(&mut self.startup_lines)
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
