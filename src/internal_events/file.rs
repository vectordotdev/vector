use metrics::{counter, gauge};
use std::borrow::Cow;
use vector_core::internal_event::{ComponentEventsDropped, InternalEvent, UNINTENTIONAL};

use crate::emit;

#[cfg(any(feature = "sources-file", feature = "sources-kubernetes_logs"))]
pub use self::source::*;

use vector_common::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct FileOpen {
    pub count: usize,
}

impl InternalEvent for FileOpen {
    fn emit(self) {
        gauge!("open_files", self.count as f64);
    }
}

#[derive(Debug)]
pub struct FileBytesSent<'a> {
    pub byte_size: usize,
    pub file: Cow<'a, str>,
}

impl InternalEvent for FileBytesSent<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes sent.",
            byte_size = %self.byte_size,
            protocol = "file",
            file = %self.file,
        );
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => "file",
            "file" => self.file.clone().into_owned(),
        );
    }
}

#[derive(Debug)]
pub struct FileIoError<'a, P> {
    pub error: std::io::Error,
    pub code: &'static str,
    pub message: &'static str,
    pub path: &'a P,
    pub dropped_events: usize,
}

impl<'a, P: std::fmt::Debug> InternalEvent for FileIoError<'a, P> {
    fn emit(self) {
        error!(
            message = %self.message,
            path = ?self.path,
            error = %self.error,
            error_code = %self.code,
            error_type = error_type::IO_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => self.code,
            "error_type" => error_type::IO_FAILED,
            "stage" => error_stage::SENDING,
        );

        if self.dropped_events > 0 {
            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: self.dropped_events,
                reason: self.message,
            });
        }
    }
}

#[cfg(any(feature = "sources-file", feature = "sources-kubernetes_logs"))]
mod source {
    use std::{io::Error, path::Path, time::Duration};

    use file_source::FileSourceInternalEvents;
    use metrics::counter;

    use super::{FileOpen, InternalEvent};
    use crate::emit;
    use vector_common::{
        internal_event::{error_stage, error_type},
        json_size::JsonSize,
    };

    #[derive(Debug)]
    pub struct FileBytesReceived<'a> {
        pub byte_size: usize,
        pub file: &'a str,
    }

    impl<'a> InternalEvent for FileBytesReceived<'a> {
        fn emit(self) {
            trace!(
                message = "Bytes received.",
                byte_size = %self.byte_size,
                protocol = "file",
                file = %self.file,
            );
            counter!(
                "component_received_bytes_total", self.byte_size as u64,
                "protocol" => "file",
                "file" => self.file.to_owned()
            );
        }
    }

    #[derive(Debug)]
    pub struct FileEventsReceived<'a> {
        pub count: usize,
        pub file: &'a str,
        pub byte_size: JsonSize,
    }

    impl InternalEvent for FileEventsReceived<'_> {
        fn emit(self) {
            trace!(
                message = "Events received.",
                count = %self.count,
                byte_size = %self.byte_size,
                file = %self.file
            );
            counter!(
                "component_received_events_total", self.count as u64,
                "file" => self.file.to_owned(),
            );
            counter!(
                "component_received_event_bytes_total", self.byte_size.get() as u64,
                "file" => self.file.to_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileChecksumFailed<'a> {
        pub file: &'a Path,
    }

    impl<'a> InternalEvent for FileChecksumFailed<'a> {
        fn emit(self) {
            warn!(
                message = "Currently ignoring file too small to fingerprint.",
                file = %self.file.display(),
            );
            counter!(
                "checksum_errors_total", 1,
                "file" => self.file.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileFingerprintReadError<'a> {
        pub file: &'a Path,
        pub error: Error,
    }

    impl<'a> InternalEvent for FileFingerprintReadError<'a> {
        fn emit(self) {
            error!(
                message = "Failed reading file for fingerprinting.",
                file = %self.file.display(),
                error = %self.error,
                error_code = "reading_fingerprint",
                error_type = error_type::READER_FAILED,
                stage = error_stage::RECEIVING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "reading_fingerprint",
                "error_type" => error_type::READER_FAILED,
                "stage" => error_stage::RECEIVING,
                "file" => self.file.to_string_lossy().into_owned(),
            );
            // deprecated
            counter!(
                "fingerprint_read_errors_total", 1,
                "file" => self.file.to_string_lossy().into_owned(),
            );
        }
    }

    const DELETION_FAILED: &str = "deletion_failed";

    #[derive(Debug)]
    pub struct FileDeleteError<'a> {
        pub file: &'a Path,
        pub error: Error,
    }

    impl<'a> InternalEvent for FileDeleteError<'a> {
        fn emit(self) {
            error!(
                message = "Failed in deleting file.",
                file = %self.file.display(),
                error = %self.error,
                error_code = DELETION_FAILED,
                error_type = error_type::COMMAND_FAILED,
                stage = error_stage::RECEIVING,
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "file" => self.file.to_string_lossy().into_owned(),
                "error_code" => DELETION_FAILED,
                "error_type" => error_type::COMMAND_FAILED,
                "stage" => error_stage::RECEIVING,
            );
            // deprecated
            counter!(
                "file_delete_errors_total", 1,
                "file" => self.file.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileDeleted<'a> {
        pub file: &'a Path,
    }

    impl<'a> InternalEvent for FileDeleted<'a> {
        fn emit(self) {
            info!(
                message = "File deleted.",
                file = %self.file.display(),
            );
            counter!(
                "files_deleted_total", 1,
                "file" => self.file.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileUnwatched<'a> {
        pub file: &'a Path,
    }

    impl<'a> InternalEvent for FileUnwatched<'a> {
        fn emit(self) {
            info!(
                message = "Stopped watching file.",
                file = %self.file.display(),
            );
            counter!(
                "files_unwatched_total", 1,
                "file" => self.file.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    struct FileWatchError<'a> {
        pub file: &'a Path,
        pub error: Error,
    }

    impl<'a> InternalEvent for FileWatchError<'a> {
        fn emit(self) {
            error!(
                message = "Failed to watch file.",
                error = %self.error,
                error_code = "watching",
                error_type = error_type::COMMAND_FAILED,
                stage = error_stage::RECEIVING,
                file = %self.file.display(),
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "watching",
                "error_type" => error_type::COMMAND_FAILED,
                "stage" => error_stage::RECEIVING,
                "file" => self.file.to_string_lossy().into_owned(),
            );
            // deprecated
            counter!(
                "file_watch_errors_total", 1,
                "file" => self.file.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileResumed<'a> {
        pub file: &'a Path,
        pub file_position: u64,
    }

    impl<'a> InternalEvent for FileResumed<'a> {
        fn emit(self) {
            info!(
                message = "Resuming to watch file.",
                file = %self.file.display(),
                file_position = %self.file_position
            );
            counter!(
                "files_resumed_total", 1,
                "file" => self.file.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileAdded<'a> {
        pub file: &'a Path,
    }

    impl<'a> InternalEvent for FileAdded<'a> {
        fn emit(self) {
            info!(
                message = "Found new file to watch.",
                file = %self.file.display(),
            );
            counter!(
                "files_added_total", 1,
                "file" => self.file.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Debug)]
    pub struct FileCheckpointed {
        pub count: usize,
        pub duration: Duration,
    }

    impl InternalEvent for FileCheckpointed {
        fn emit(self) {
            debug!(
                message = "Files checkpointed.",
                count = %self.count,
                duration_ms = self.duration.as_millis() as u64,
            );
            counter!("checkpoints_total", self.count as u64);
        }
    }

    #[derive(Debug)]
    pub struct FileCheckpointWriteError {
        pub error: Error,
    }

    impl InternalEvent for FileCheckpointWriteError {
        fn emit(self) {
            error!(
                message = "Failed writing checkpoints.",
                error = %self.error,
                error_code = "writing_checkpoints",
                error_type = error_type::WRITER_FAILED,
                stage = error_stage::RECEIVING,
                internal_log_rate_limit = true,
            );
            counter!("checkpoint_write_errors_total", 1);
            counter!(
                "component_errors_total", 1,
                "error_code" => "writing_checkpoints",
                "error_type" => error_type::WRITER_FAILED,
                "stage" => error_stage::RECEIVING,
            );
        }
    }

    #[derive(Debug)]
    pub struct PathGlobbingError<'a> {
        pub path: &'a Path,
        pub error: &'a Error,
    }

    impl<'a> InternalEvent for PathGlobbingError<'a> {
        fn emit(self) {
            error!(
                message = "Failed to glob path.",
                error = %self.error,
                error_code = "globbing",
                error_type = error_type::READER_FAILED,
                stage = error_stage::RECEIVING,
                path = %self.path.display(),
                internal_log_rate_limit = true,
            );
            counter!(
                "component_errors_total", 1,
                "error_code" => "globbing",
                "error_type" => error_type::READER_FAILED,
                "stage" => error_stage::RECEIVING,
                "path" => self.path.to_string_lossy().into_owned(),
            );
            // deprecated
            counter!(
                "glob_errors_total", 1,
                "path" => self.path.to_string_lossy().into_owned(),
            );
        }
    }

    #[derive(Clone)]
    pub struct FileSourceInternalEventsEmitter;

    impl FileSourceInternalEvents for FileSourceInternalEventsEmitter {
        fn emit_file_added(&self, file: &Path) {
            emit!(FileAdded { file });
        }

        fn emit_file_resumed(&self, file: &Path, file_position: u64) {
            emit!(FileResumed {
                file,
                file_position
            });
        }

        fn emit_file_watch_error(&self, file: &Path, error: Error) {
            emit!(FileWatchError { file, error });
        }

        fn emit_file_unwatched(&self, file: &Path) {
            emit!(FileUnwatched { file });
        }

        fn emit_file_deleted(&self, file: &Path) {
            emit!(FileDeleted { file });
        }

        fn emit_file_delete_error(&self, file: &Path, error: Error) {
            emit!(FileDeleteError { file, error });
        }

        fn emit_file_fingerprint_read_error(&self, file: &Path, error: Error) {
            emit!(FileFingerprintReadError { file, error });
        }

        fn emit_file_checksum_failed(&self, file: &Path) {
            emit!(FileChecksumFailed { file });
        }

        fn emit_file_checkpointed(&self, count: usize, duration: Duration) {
            emit!(FileCheckpointed { count, duration });
        }

        fn emit_file_checkpoint_write_error(&self, error: Error) {
            emit!(FileCheckpointWriteError { error });
        }

        fn emit_files_open(&self, count: usize) {
            emit!(FileOpen { count });
        }

        fn emit_path_globbing_failed(&self, path: &Path, error: &Error) {
            emit!(PathGlobbingError { path, error });
        }
    }
}
