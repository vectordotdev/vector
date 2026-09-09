use std::{io::Error, path::Path, time::Duration};

use bytes::BytesMut;

/// Every internal event in this crate has a corresponding
/// method in this trait which should emit the event.
pub trait FileSourceInternalEvents: Send + Sync + Clone + 'static {
    fn emit_file_added(&self, path: &Path);

    fn emit_file_resumed(&self, path: &Path, file_position: u64);

    fn emit_file_watch_error(&self, path: &Path, error: Error);

    fn emit_file_unwatched(&self, path: &Path, reached_eof: bool);

    fn emit_file_deleted(&self, path: &Path);

    fn emit_file_delete_error(&self, path: &Path, error: Error);

    fn emit_file_fingerprint_read_error(&self, path: &Path, error: Error);

    fn emit_file_checkpointed(&self, count: usize, duration: Duration);

    fn emit_file_checksum_failed(&self, path: &Path);

    fn emit_file_checkpoint_write_error(&self, error: Error);

    /// Number of files with an actually-open file handle (i.e. `Active`
    /// watchers). Distinct from the total number of tracked files, which may
    /// also include `Idle` watchers that hold no handle at all.
    fn emit_files_open(&self, count: usize);

    /// Number of tracked files currently in the passive `Idle` state: no open
    /// file handle, checkpoint retained, polled only via cheap `fs::metadata`
    /// stats. See <https://github.com/vectordotdev/vector/issues/3567>.
    fn emit_files_idle(&self, count: usize);

    fn emit_path_globbing_failed(&self, path: &Path, error: &Error);

    fn emit_file_line_too_long(
        &self,
        truncated_bytes: &BytesMut,
        configured_limit: usize,
        encountered_size_so_far: usize,
    );

    /// Emitted when the OS-level filesystem event watcher (if in use) reports that its
    /// internal event queue overflowed, meaning some events may have been silently dropped.
    /// Implementors should log this loudly, since it means the event-driven discovery path
    /// may have missed file creations/modifications until the next reconciliation pass.
    fn emit_file_watch_events_overflowed(&self) {}

    /// Emitted when the OS-level filesystem event watcher itself fails (e.g. the watched
    /// directory disappears, or the OS notification API errors out). The event-driven
    /// discovery path will keep relying on the periodic reconciliation pass until watching
    /// can be re-established.
    fn emit_file_watch_backend_error(&self, _error: &Error) {}

    /// Emitted once at startup (or when watched directories change) to report how many
    /// directories are being watched via OS-level notifications.
    fn emit_file_watch_directories(&self, _count: usize) {}
}
