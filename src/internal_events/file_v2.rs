use vector_lib::{configurable::configurable_component, internal_event::InternalEvent};

pub use self::source::*;
pub use super::file::{
    FileAdded, FileCheckpointed, FileChecksumFailed, FileDeleted, FileOpen, FileResumed,
    FileUnwatched,
};

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

mod source {
    use std::{io::Error, path::Path, time::Duration};

    use vector_lib::file_source_common::internal_events::FileSourceInternalEvents;
    use vector_lib::{counter, internal_event::CounterName};

    use crate::internal_events::FileLineTooBigError;

    use super::{
        FileAdded, FileCheckpointed, FileChecksumFailed, FileDeleted, FileOpen, FileResumed,
        FileUnwatched, InternalEvent,
    };
    use vector_lib::emit;
    use vector_lib::internal_event::{error_stage, error_type};

    use metrics::Counter;
    use vector_lib::internal_event::{ByteSize, CountByteSize};

    vector_lib::registered_event!(
        FileBytesReceived {
            file: Option<String>,
        } => {
            bytes: Counter = match self.file {
                Some(file) => counter!(CounterName::ComponentReceivedBytesTotal, "protocol" => "file_v2", "file" => file),
                None => counter!(CounterName::ComponentReceivedBytesTotal, "protocol" => "file_v2"),
            },
        }

        fn emit(&self, data: ByteSize) {
            self.bytes.increment(data.0 as u64);
        }
    );

    vector_lib::registered_event!(
        FileEventsReceived {
            file: Option<String>,
        } => {
            events: Counter = match self.file.as_ref() {
                Some(file) => counter!(CounterName::ComponentReceivedEventsTotal, "file" => file.clone()),
                None => counter!(CounterName::ComponentReceivedEventsTotal),
            },
            event_bytes: Counter = match self.file {
                Some(file) => counter!(CounterName::ComponentReceivedEventBytesTotal, "file" => file),
                None => counter!(CounterName::ComponentReceivedEventBytesTotal),
            },
        }

        fn emit(&self, data: CountByteSize) {
            self.events.increment(data.0 as u64);
            self.event_bytes.increment(data.1.get() as u64);
        }
    );

    #[derive(Debug, vector_lib::NamedInternalEvent)]
    pub struct FileFingerprintReadError<'a> {
        pub file: &'a Path,
        pub error: Error,
        pub include_file_metric_tag: bool,
    }

    impl InternalEvent for FileFingerprintReadError<'_> {
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
                    CounterName::ComponentErrorsTotal,
                    "error_code" => "reading_fingerprint",
                    "error_type" => error_type::READER_FAILED,
                    "stage" => error_stage::RECEIVING,
                    "file" => self.file.to_string_lossy().into_owned(),
                )
            } else {
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_code" => "reading_fingerprint",
                    "error_type" => error_type::READER_FAILED,
                    "stage" => error_stage::RECEIVING,
                )
            }
            .increment(1);
        }
    }

    const DELETION_FAILED: &str = "deletion_failed";

    #[derive(Debug, vector_lib::NamedInternalEvent)]
    pub struct FileDeleteError<'a> {
        pub file: &'a Path,
        pub error: Error,
        pub include_file_metric_tag: bool,
    }

    impl InternalEvent for FileDeleteError<'_> {
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
                    CounterName::ComponentErrorsTotal,
                    "file" => self.file.to_string_lossy().into_owned(),
                    "error_code" => DELETION_FAILED,
                    "error_type" => error_type::COMMAND_FAILED,
                    "stage" => error_stage::RECEIVING,
                )
            } else {
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_code" => DELETION_FAILED,
                    "error_type" => error_type::COMMAND_FAILED,
                    "stage" => error_stage::RECEIVING,
                )
            }
            .increment(1);
        }
    }

    #[derive(Debug, vector_lib::NamedInternalEvent)]
    struct FileWatchError<'a> {
        pub file: &'a Path,
        pub error: Error,
        pub include_file_metric_tag: bool,
    }

    impl InternalEvent for FileWatchError<'_> {
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
                    CounterName::ComponentErrorsTotal,
                    "error_code" => "watching",
                    "error_type" => error_type::COMMAND_FAILED,
                    "stage" => error_stage::RECEIVING,
                    "file" => self.file.to_string_lossy().into_owned(),
                )
            } else {
                counter!(
                    CounterName::ComponentErrorsTotal,
                    "error_code" => "watching",
                    "error_type" => error_type::COMMAND_FAILED,
                    "stage" => error_stage::RECEIVING,
                )
            }
            .increment(1);
        }
    }

    #[derive(Debug, vector_lib::NamedInternalEvent)]
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
                CounterName::ComponentErrorsTotal,
                "error_code" => "writing_checkpoints",
                "error_type" => error_type::WRITER_FAILED,
                "stage" => error_stage::RECEIVING,
            )
            .increment(1);
        }
    }

    #[derive(Debug, vector_lib::NamedInternalEvent)]
    pub struct PathGlobbingError<'a> {
        pub path: &'a Path,
        pub error: &'a Error,
    }

    impl InternalEvent for PathGlobbingError<'_> {
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
                CounterName::ComponentErrorsTotal,
                "error_code" => "globbing",
                "error_type" => error_type::READER_FAILED,
                "stage" => error_stage::RECEIVING,
            )
            .increment(1);
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

        fn emit_file_unwatched(&self, file: &Path, reached_eof: bool) {
            emit!(FileUnwatched {
                file,
                include_file_metric_tag: self.include_file_metric_tag,
                reached_eof
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

        fn emit_file_line_too_long(
            &self,
            truncated_bytes: &bytes::BytesMut,
            configured_limit: usize,
            encountered_size_so_far: usize,
        ) {
            emit!(FileLineTooBigError {
                truncated_bytes,
                configured_limit,
                encountered_size_so_far
            });
        }
    }
}
