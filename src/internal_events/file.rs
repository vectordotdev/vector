use metrics::{counter, gauge};
use std::borrow::Cow;
use vector_lib::{
    configurable::configurable_component,
    internal_event::{ComponentEventsDropped, InternalEvent, UNINTENTIONAL},
};

#[cfg(any(feature = "sources-file", feature = "sources-kubernetes_logs"))]
pub use self::source::*;

use vector_lib::internal_event::{error_stage, error_type};

/// Configuration of internal metrics for file-based components.
#[configurable_component]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct FileInternalMetricsConfig {
    /// Whether or not to include the "file" tag on the component's corresponding internal metrics.
    ///
    /// This is useful for distinguishing between different files while monitoring. However, the tag's
    /// cardinality is unbounded.
    #[serde(default = "crate::serde::default_false")]
    pub include_file_tag: bool,
}

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
    pub include_file_metric_tag: bool,
}

impl InternalEvent for FileBytesSent<'_> {
    fn emit(self) {
        trace!(
            message = "Bytes sent.",
            byte_size = %self.byte_size,
            protocol = "file",
            file = %self.file,
        );
        if self.include_file_metric_tag {
            counter!(
                "component_sent_bytes_total", self.byte_size as u64,
                "protocol" => "file",
                "file" => self.file.clone().into_owned(),
            );
        } else {
            counter!(
                "component_sent_bytes_total", self.byte_size as u64,
                "protocol" => "file",
            );
        }
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

    use metrics::counter;
    use vector_lib::file_source::FileSourceInternalEvents;

    use super::{FileOpen, InternalEvent};
    use vector_lib::emit;
    use vector_lib::{
        internal_event::{error_stage, error_type},
        json_size::JsonSize,
    };

    #[derive(Debug)]
    pub struct FileBytesReceived<'a> {
        pub byte_size: usize,
        pub file: &'a str,
        pub include_file_metric_tag: bool,
    }

    impl<'a> InternalEvent for FileBytesReceived<'a> {
        fn emit(self) {
            trace!(
                message = "Bytes received.",
                byte_size = %self.byte_size,
                protocol = "file",
                file = %self.file,
            );
            if self.include_file_metric_tag {
                counter!(
                    "component_received_bytes_total", self.byte_size as u64,
                    "protocol" => "file",
                    "file" => self.file.to_owned()
                );
            } else {
                counter!(
                    "component_received_bytes_total", self.byte_size as u64,
                    "protocol" => "file",
                );
            }
        }
    }

    #[derive(Debug)]
    pub struct FileEventsReceived<'a> {
        pub count: usize,
        pub file: &'a str,
        pub byte_size: JsonSize,
        pub include_file_metric_tag: bool,
    }

    impl InternalEvent for FileEventsReceived<'_> {
        fn emit(self) {
            trace!(
                message = "Events received.",
                count = %self.count,
                byte_size = %self.byte_size,
                file = %self.file
            );
            if self.include_file_metric_tag {
                counter!(
                    "component_received_events_total", self.count as u64,
                    "file" => self.file.to_owned(),
                );
                counter!(
                    "component_received_event_bytes_total", self.byte_size.get() as u64,
                    "file" => self.file.to_owned(),
                );
            } else {
                counter!("component_received_events_total", self.count as u64);
                counter!(
                    "component_received_event_bytes_total",
                    self.byte_size.get() as u64,
                );
            }
        }
    }

    #[derive(Debug)]
    pub struct FileChecksumFailed<'a> {
        pub file: &'a Path,
        pub include_file_metric_tag: bool,
    }

    impl<'a> InternalEvent for FileChecksumFailed<'a> {
        fn emit(self) {
            warn!(
                message = "Currently ignoring file too small to fingerprint.",
                file = %self.file.display(),
            );
            if self.include_file_metric_tag {
                counter!(
                    "checksum_errors_total", 1,
                    "file" => self.file.to_string_lossy().into_owned(),
                );
            } else {
                counter!("checksum_errors_total", 1);
            }
        }
    }

    #[derive(Debug)]
    pub struct FileFingerprintReadError<'a> {
        pub file: &'a Path,
        pub error: Error,
        pub include_file_metric_tag: bool,
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
            if self.include_file_metric_tag {
                counter!(
                    "component_errors_total", 1,
                    "error_code" => "reading_fingerprint",
                    "error_type" => error_type::READER_FAILED,
                    "stage" => error_stage::RECEIVING,
                    "file" => self.file.to_string_lossy().into_owned(),
                );
            } else {
                counter!(
                    "component_errors_total", 1,
                    "error_code" => "reading_fingerprint",
                    "error_type" => error_type::READER_FAILED,
                    "stage" => error_stage::RECEIVING,
                );
            }
        }
    }

    const DELETION_FAILED: &str = "deletion_failed";

    #[derive(Debug)]
    pub struct FileDeleteError<'a> {
        pub file: &'a Path,
        pub error: Error,
        pub include_file_metric_tag: bool,
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
            if self.include_file_metric_tag {
                counter!(
                    "component_errors_total", 1,
                    "file" => self.file.to_string_lossy().into_owned(),
                    "error_code" => DELETION_FAILED,
                    "error_type" => error_type::COMMAND_FAILED,
                    "stage" => error_stage::RECEIVING,
                );
            } else {
                counter!(
                    "component_errors_total", 1,
                    "error_code" => DELETION_FAILED,
                    "error_type" => error_type::COMMAND_FAILED,
                    "stage" => error_stage::RECEIVING,
                );
            }
        }
    }

    #[derive(Debug)]
    pub struct FileDeleted<'a> {
        pub file: &'a Path,
        pub include_file_metric_tag: bool,
    }

    impl<'a> InternalEvent for FileDeleted<'a> {
        fn emit(self) {
            info!(
                message = "File deleted.",
                file = %self.file.display(),
            );
            if self.include_file_metric_tag {
                counter!(
                    "files_deleted_total", 1,
                    "file" => self.file.to_string_lossy().into_owned(),
                );
            } else {
                counter!("files_deleted_total", 1);
            }
        }
    }

    #[derive(Debug)]
    pub struct FileUnwatched<'a> {
        pub file: &'a Path,
        pub include_file_metric_tag: bool,
    }

    impl<'a> InternalEvent for FileUnwatched<'a> {
        fn emit(self) {
            info!(
                message = "Stopped watching file.",
                file = %self.file.display(),
            );
            if self.include_file_metric_tag {
                counter!(
                    "files_unwatched_total", 1,
                    "file" => self.file.to_string_lossy().into_owned(),
                );
            } else {
                counter!("files_unwatched_total", 1);
            }
        }
    }

    #[derive(Debug)]
    struct FileWatchError<'a> {
        pub file: &'a Path,
        pub error: Error,
        pub include_file_metric_tag: bool,
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
            if self.include_file_metric_tag {
                counter!(
                    "component_errors_total", 1,
                    "error_code" => "watching",
                    "error_type" => error_type::COMMAND_FAILED,
                    "stage" => error_stage::RECEIVING,
                    "file" => self.file.to_string_lossy().into_owned(),
                );
            } else {
                counter!(
                    "component_errors_total", 1,
                    "error_code" => "watching",
                    "error_type" => error_type::COMMAND_FAILED,
                    "stage" => error_stage::RECEIVING,
                );
            }
        }
    }

    #[derive(Debug)]
    pub struct FileResumed<'a> {
        pub file: &'a Path,
        pub file_position: u64,
        pub include_file_metric_tag: bool,
    }

    impl<'a> InternalEvent for FileResumed<'a> {
        fn emit(self) {
            info!(
                message = "Resuming to watch file.",
                file = %self.file.display(),
                file_position = %self.file_position
            );
            if self.include_file_metric_tag {
                counter!(
                    "files_resumed_total", 1,
                    "file" => self.file.to_string_lossy().into_owned(),
                );
            } else {
                counter!("files_resumed_total", 1);
            }
        }
    }

    #[derive(Debug)]
    pub struct FileAdded<'a> {
        pub file: &'a Path,
        pub include_file_metric_tag: bool,
    }

    impl<'a> InternalEvent for FileAdded<'a> {
        fn emit(self) {
            info!(
                message = "Found new file to watch.",
                file = %self.file.display(),
            );
            if self.include_file_metric_tag {
                counter!(
                    "files_added_total", 1,
                    "file" => self.file.to_string_lossy().into_owned(),
                );
            } else {
                counter!("files_added_total", 1);
            }
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
            );
        }
    }

    #[derive(Clone)]
    pub struct FileSourceInternalEventsEmitter {
        pub include_file_metric_tag: bool,
    }

    impl FileSourceInternalEvents for FileSourceInternalEventsEmitter {
        fn emit_file_added(&self, file: &Path) {
            emit!(FileAdded {
                file,
                include_file_metric_tag: self.include_file_metric_tag
            });
        }

        fn emit_file_resumed(&self, file: &Path, file_position: u64) {
            emit!(FileResumed {
                file,
                file_position,
                include_file_metric_tag: self.include_file_metric_tag
            });
        }

        fn emit_file_watch_error(&self, file: &Path, error: Error) {
            emit!(FileWatchError {
                file,
                error,
                include_file_metric_tag: self.include_file_metric_tag
            });
        }

        fn emit_file_unwatched(&self, file: &Path) {
            emit!(FileUnwatched {
                file,
                include_file_metric_tag: self.include_file_metric_tag
            });
        }

        fn emit_file_deleted(&self, file: &Path) {
            emit!(FileDeleted {
                file,
                include_file_metric_tag: self.include_file_metric_tag
            });
        }

        fn emit_file_delete_error(&self, file: &Path, error: Error) {
            emit!(FileDeleteError {
                file,
                error,
                include_file_metric_tag: self.include_file_metric_tag
            });
        }

        fn emit_file_fingerprint_read_error(&self, file: &Path, error: Error) {
            emit!(FileFingerprintReadError {
                file,
                error,
                include_file_metric_tag: self.include_file_metric_tag
            });
        }

        fn emit_file_checksum_failed(&self, file: &Path) {
            emit!(FileChecksumFailed {
                file,
                include_file_metric_tag: self.include_file_metric_tag
            });
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
