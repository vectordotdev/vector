use super::InternalEvent;
use file_source::FileSourceInternalEvents;
use metrics::counter;
use std::io::Error;
use std::path::Path;

#[derive(Debug)]
pub struct FileEventReceived<'a> {
    pub file: &'a str,
    pub byte_size: usize,
}

impl InternalEvent for FileEventReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Received one event.",
            file = %self.file,
            byte_size = %self.byte_size
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "events_processed", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
        counter!(
            "bytes_processed", self.byte_size as u64,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileChecksumFailed<'a> {
    pub path: &'a Path,
}

impl<'a> InternalEvent for FileChecksumFailed<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "currently ignoring file too small for fingerprinting.",
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "checksum_errors", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileFingerprintReadFailed<'a> {
    pub path: &'a Path,
    pub error: Error,
}

impl<'a> InternalEvent for FileFingerprintReadFailed<'a> {
    fn emit_logs(&self) {
        error!(
            message = "failed reading file for fingerprinting.",
            path = ?self.path,
            error = ?self.error,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "fingerprint_read_errors", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileDeleteFailed<'a> {
    pub path: &'a Path,
    pub error: Error,
}

impl<'a> InternalEvent for FileDeleteFailed<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "failed in deleting file.",
            path = ?self.path,
            error = ?self.error,
            rate_limit_secs = 1
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "file_delete_errors", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileDeleted<'a> {
    pub path: &'a Path,
}

impl<'a> InternalEvent for FileDeleted<'a> {
    fn emit_logs(&self) {
        info!(
            message = "file deleted.",
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "files_deleted", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileUnwatched<'a> {
    pub path: &'a Path,
}

impl<'a> InternalEvent for FileUnwatched<'a> {
    fn emit_logs(&self) {
        info!(
            message = "stopped watching file.",
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "files_unwatched", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileWatchFailed<'a> {
    pub path: &'a Path,
    pub error: Error,
}

impl<'a> InternalEvent for FileWatchFailed<'a> {
    fn emit_logs(&self) {
        error!(
            message = "failed to watch file.",
            path = ?self.path,
            error = ?self.error
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "file_watch_errors", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileResumed<'a> {
    pub path: &'a Path,
    pub file_position: u64,
}

impl<'a> InternalEvent for FileResumed<'a> {
    fn emit_logs(&self) {
        info!(
            message = "resuming to watch file.",
            path = ?self.path,
            file_position = %self.file_position
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "files_resumed", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileAdded<'a> {
    pub path: &'a Path,
}

impl<'a> InternalEvent for FileAdded<'a> {
    fn emit_logs(&self) {
        info!(
            message = "found new file to watch.",
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "files_added", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileCheckpointed {
    pub count: usize,
}

impl InternalEvent for FileCheckpointed {
    fn emit_logs(&self) {
        debug!(message = "files checkpointed.", count = %self.count);
    }

    fn emit_metrics(&self) {
        counter!(
            "checkpoints", self.count as u64,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

#[derive(Debug)]
pub struct FileCheckpointWriteFailed {
    pub error: Error,
}

impl InternalEvent for FileCheckpointWriteFailed {
    fn emit_logs(&self) {
        warn!(message = "failed writing checkpoints.", error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!(
            "checkpoint_write_errors", 1,
            "component_kind" => "source",
            "component_type" => "file",
        );
    }
}

pub struct FileSourceInternalEventsEmitter;

impl FileSourceInternalEvents for FileSourceInternalEventsEmitter {
    fn emit_file_added(&self, path: &Path) {
        emit!(FileAdded { path });
    }

    fn emit_file_resumed(&self, path: &Path, file_position: u64) {
        emit!(FileResumed {
            path,
            file_position
        });
    }

    fn emit_file_watch_failed(&self, path: &Path, error: Error) {
        emit!(FileWatchFailed { path, error });
    }

    fn emit_file_unwatched(&self, path: &Path) {
        emit!(FileUnwatched { path });
    }

    fn emit_file_deleted(&self, path: &Path) {
        emit!(FileDeleted { path });
    }

    fn emit_file_delete_failed(&self, path: &Path, error: Error) {
        emit!(FileDeleteFailed { path, error });
    }

    fn emit_file_fingerprint_read_failed(&self, path: &Path, error: Error) {
        emit!(FileFingerprintReadFailed { path, error });
    }

    fn emit_file_checksum_failed(&self, path: &Path) {
        emit!(FileChecksumFailed { path });
    }

    fn emit_file_checkpointed(&self, count: usize) {
        emit!(FileCheckpointed { count });
    }

    fn emit_file_checkpoint_write_failed(&self, error: Error) {
        emit!(FileCheckpointWriteFailed { error });
    }
}
