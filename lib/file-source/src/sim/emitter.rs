use std::{
    io::Error,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use bytes::BytesMut;
use file_source_common::FileSourceInternalEvents;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnwatchEvent {
    pub path: PathBuf,
    pub reached_eof: bool,
    pub bytes_unread: Option<u64>,
}

#[derive(Debug, Default)]
pub struct RecordedEvents {
    pub added: Vec<PathBuf>,
    pub resumed: Vec<(PathBuf, u64)>,
    pub unwatched: Vec<UnwatchEvent>,
    pub deleted: Vec<PathBuf>,
    pub delete_errors: Vec<PathBuf>,
    pub checksum_failed: Vec<PathBuf>,
    pub watch_errors: Vec<PathBuf>,
    pub fingerprint_read_errors: Vec<PathBuf>,
    pub line_too_long: Vec<BytesMut>,
    pub files_open: Vec<usize>,
    pub checkpointed: Vec<usize>,
}

/// [`FileSourceInternalEvents`] implementation that records all emitted events
/// for deterministic assertions.
#[derive(Debug, Clone, Default)]
pub struct RecordingEmitter {
    events: Arc<Mutex<RecordedEvents>>,
}

impl RecordingEmitter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_events<R>(&self, f: impl FnOnce(&RecordedEvents) -> R) -> R {
        f(&self.events.lock().unwrap())
    }
}

impl FileSourceInternalEvents for RecordingEmitter {
    fn emit_file_added(&self, path: &Path) {
        self.events.lock().unwrap().added.push(path.to_path_buf());
    }

    fn emit_file_resumed(&self, path: &Path, file_position: u64) {
        self.events
            .lock()
            .unwrap()
            .resumed
            .push((path.to_path_buf(), file_position));
    }

    fn emit_file_watch_error(&self, path: &Path, _error: Error) {
        self.events
            .lock()
            .unwrap()
            .watch_errors
            .push(path.to_path_buf());
    }

    fn emit_file_unwatched(&self, path: &Path, reached_eof: bool, bytes_unread: Option<u64>) {
        self.events.lock().unwrap().unwatched.push(UnwatchEvent {
            path: path.to_path_buf(),
            reached_eof,
            bytes_unread,
        });
    }

    fn emit_file_deleted(&self, path: &Path) {
        self.events.lock().unwrap().deleted.push(path.to_path_buf());
    }

    fn emit_file_delete_error(&self, path: &Path, _error: Error) {
        self.events
            .lock()
            .unwrap()
            .delete_errors
            .push(path.to_path_buf());
    }

    fn emit_file_fingerprint_read_error(&self, path: &Path, _error: Error) {
        self.events
            .lock()
            .unwrap()
            .fingerprint_read_errors
            .push(path.to_path_buf());
    }

    fn emit_file_checksum_failed(&self, path: &Path) {
        self.events
            .lock()
            .unwrap()
            .checksum_failed
            .push(path.to_path_buf());
    }

    fn emit_file_checkpointed(&self, count: usize, _duration: Duration) {
        self.events.lock().unwrap().checkpointed.push(count);
    }

    fn emit_file_checkpoint_write_error(&self, _error: Error) {}

    fn emit_files_open(&self, count: usize) {
        self.events.lock().unwrap().files_open.push(count);
    }

    fn emit_path_globbing_failed(&self, _path: &Path, _error: &Error) {}

    fn emit_file_line_too_long(
        &self,
        truncated_bytes: &BytesMut,
        _configured_limit: usize,
        _encountered_size_so_far: usize,
    ) {
        self.events
            .lock()
            .unwrap()
            .line_too_long
            .push(truncated_bytes.clone());
    }
}
