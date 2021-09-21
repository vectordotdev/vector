use metrics::gauge;
use vector_core::internal_event::InternalEvent;

#[cfg(any(feature = "sources-file", feature = "sources-kubernetes_logs"))]
pub use self::source::*;

#[derive(Debug)]
pub struct FileOpen {
    pub count: usize,
}

impl InternalEvent for FileOpen {
    fn emit_metrics(&self) {
        gauge!("open_files", self.count as f64);
    }
}

#[cfg(any(feature = "sources-file", feature = "sources-kubernetes_logs"))]
mod source {
    use super::{FileOpen, InternalEvent};
    use file_source::FileSourceInternalEvents;
    use metrics::counter;
    use std::{io::Error, path::Path, time::Duration};

    #[derive(Debug)]
    pub struct FileBytesReceived<'a> {
        pub byte_size: usize,
        pub path: &'a str,
    }

    impl<'a> InternalEvent for FileBytesReceived<'a> {
        fn emit_logs(&self) {
            trace!(
                message = "Bytes received.",
                byte_size = %self.byte_size,
                protocol = "file",
                path = %self.path,
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "component_received_bytes_total", self.byte_size as u64,
                "protocol" => "file",
                "file" => self.path.to_string()
            );
        }
    }

    #[derive(Debug)]
    pub struct FileEventsReceived<'a> {
        pub file: &'a str,
        pub byte_size: usize,
    }

    impl InternalEvent for FileEventsReceived<'_> {
        fn emit_logs(&self) {
            trace!(
                message = "Received one event.",
                file = %self.file,
                byte_size = %self.byte_size
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "events_in_total", 1,
                "file" => self.file.to_owned(),
            );
            counter!(
                "processed_bytes_total", self.byte_size as u64,
                "file" => self.file.to_owned(),
            );
            counter!(
                "component_received_events_total", 1,
                "file" => self.file.to_owned(),
            );
            counter!(
                "component_received_event_bytes_total", self.byte_size as u64,
                "file" => self.file.to_owned(),
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
                message = "Currently ignoring file too small to fingerprint.",
                path = %self.path.display(),
            )
        }

        fn emit_metrics(&self) {
            counter!(
                "checksum_errors_total", 1,
                "file" => self.path.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileFingerprintReadError<'a> {
        pub path: &'a Path,
        pub error: Error,
    }

    impl<'a> InternalEvent for FileFingerprintReadError<'a> {
        fn emit_logs(&self) {
            error!(
                message = "Failed reading file for fingerprinting.",
                path = %self.path.display(),
                error_type = "read_failed",
                error = %self.error,
                stage = "receiving",
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "fingerprint_read_errors_total", 1,
                "file" => self.path.to_string_lossy().into_owned(),
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => "read_failed",
                "file" => self.path.to_string_lossy().into_owned(),
                "stage" => "receiving",
            );
        }
    }

    #[derive(Debug)]
    pub struct FileDeleteError<'a> {
        pub path: &'a Path,
        pub error: Error,
    }

    impl<'a> InternalEvent for FileDeleteError<'a> {
        fn emit_logs(&self) {
            warn!(
                message = "Failed in deleting file.",
                path = %self.path.display(),
                error = %self.error,
                internal_log_rate_secs = 1
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "file_delete_errors_total", 1,
                "file" => self.path.to_string_lossy().into_owned(),
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => "delete_failed",
                "file" => self.path.to_string_lossy().into_owned(),
                "stage" => "receiving"
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
                message = "File deleted.",
                path = %self.path.display(),
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "files_deleted_total", 1,
                "file" => self.path.to_string_lossy().into_owned(),
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
                message = "Stopped watching file.",
                path = %self.path.display(),
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "files_unwatched_total", 1,
                "file" => self.path.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileWatchError<'a> {
        pub path: &'a Path,
        pub error: Error,
    }

    impl<'a> InternalEvent for FileWatchError<'a> {
        fn emit_logs(&self) {
            error!(
                message = "Failed to watch file.",
                path = %self.path.display(),
                error_type = "watch_failed",
                error = %self.error,
                stage = "receiving"
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "file_watch_errors_total", 1,
                "file" => self.path.to_string_lossy().into_owned(),
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => "watch_failed",
                "file" => self.path.to_string_lossy().into_owned(),
                "stage" => "receiving"
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
                message = "Resuming to watch file.",
                path = %self.path.display(),
                file_position = %self.file_position
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "files_resumed_total", 1,
                "file" => self.path.to_string_lossy().into_owned(),
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
                message = "Found new file to watch.",
                path = %self.path.display(),
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "files_added_total", 1,
                "file" => self.path.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileCheckpointed {
        pub count: usize,
        pub duration: Duration,
    }

    impl InternalEvent for FileCheckpointed {
        fn emit_logs(&self) {
            debug!(
                message = "Files checkpointed.",
                count = %self.count,
                duration_ms = self.duration.as_millis() as u64,
            );
        }

        fn emit_metrics(&self) {
            counter!("checkpoints_total", self.count as u64);
        }
    }

    #[derive(Debug)]
    pub struct FileCheckpointWriteError {
        pub error: Error,
    }

    impl InternalEvent for FileCheckpointWriteError {
        fn emit_logs(&self) {
            error!(
                message = "Failed writing checkpoints.",
                error_type = "write_error",
                error = %self.error,
                stage = "receiving"
            );
        }

        fn emit_metrics(&self) {
            counter!("checkpoint_write_errors_total", 1);
            counter!(
                "component_errors_total", 1,
                "error_type" => "write_error",
                "stage" => "receiving"
            );
        }
    }

    #[derive(Debug)]
    pub struct PathGlobbingError<'a> {
        pub path: &'a Path,
        pub error: &'a Error,
    }

    impl<'a> InternalEvent for PathGlobbingError<'a> {
        fn emit_logs(&self) {
            error!(
                message = "Failed to glob path.",
                path = %self.path.display(),
                error_type = "glob_failed",
                error = %self.error,
                stage = "receiving"
            );
        }

        fn emit_metrics(&self) {
            counter!(
                "glob_errors_total", 1,
                "path" => self.path.to_string_lossy().into_owned(),
            );
            counter!(
                "component_errors_total", 1,
                "error_type" => "glob_failed",
                "file" => self.path.to_string_lossy().into_owned(),
                "stage" => "receiving"
            );
        }
    }

    #[derive(Clone)]
    pub struct FileSourceInternalEventsEmitter;

    impl FileSourceInternalEvents for FileSourceInternalEventsEmitter {
        fn emit_file_added(&self, path: &Path) {
            emit!(&FileAdded { path });
        }

        fn emit_file_resumed(&self, path: &Path, file_position: u64) {
            emit!(&FileResumed {
                path,
                file_position
            });
        }

        fn emit_file_watch_error(&self, path: &Path, error: Error) {
            emit!(&FileWatchError { path, error });
        }

        fn emit_file_unwatched(&self, path: &Path) {
            emit!(&FileUnwatched { path });
        }

        fn emit_file_deleted(&self, path: &Path) {
            emit!(&FileDeleted { path });
        }

        fn emit_file_delete_error(&self, path: &Path, error: Error) {
            emit!(&FileDeleteError { path, error });
        }

        fn emit_file_fingerprint_read_error(&self, path: &Path, error: Error) {
            emit!(&FileFingerprintReadError { path, error });
        }

        fn emit_file_checksum_failed(&self, path: &Path) {
            emit!(&FileChecksumFailed { path });
        }

        fn emit_file_checkpointed(&self, count: usize, duration: Duration) {
            emit!(&FileCheckpointed { count, duration });
        }

        fn emit_file_checkpoint_write_error(&self, error: Error) {
            emit!(&FileCheckpointWriteError { error });
        }

        fn emit_files_open(&self, count: usize) {
            emit!(&FileOpen { count });
        }

        fn emit_path_globbing_failed(&self, path: &Path, error: &Error) {
            emit!(&PathGlobbingError { path, error });
        }
    }
}
