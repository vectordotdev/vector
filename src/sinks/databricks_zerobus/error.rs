//! Error types for the Zerobus sink.

use databricks_zerobus_ingest_sdk::ZerobusError;
use snafu::Snafu;
use vector_lib::event::EventStatus;

/// Errors that can occur when using the Zerobus sink.
#[derive(Debug, Snafu)]
#[allow(clippy::enum_variant_names)]
pub enum ZerobusSinkError {
    /// Configuration validation failed.
    #[snafu(display("Configuration error: {}", message))]
    ConfigError { message: String },

    /// Event encoding failed.
    #[snafu(display("Encoding error: {}", message))]
    EncodingError { message: String },

    /// Zerobus SDK error.
    #[snafu(display("Zerobus error: {}", source))]
    ZerobusError { source: ZerobusError },

    /// Stream initialization failed.
    #[snafu(display("Stream initialization failed: {}", source))]
    StreamInitError { source: ZerobusError },

    /// Record ingestion failed.
    #[snafu(display("Record ingestion failed: {}", source))]
    IngestionError { source: ZerobusError },

    /// The SDK returned no offset to wait on, so server acceptance of the
    /// batch cannot be confirmed. Treated as non-retryable.
    #[snafu(display("Zerobus ingest returned no offset; cannot confirm server acceptance"))]
    MissingAckOffset,

    /// The shared stream was closed concurrently (by shutdown or retry-driven
    /// replacement) before this ingest could run. Retryable: the next attempt
    /// will create a fresh stream via `get_or_create_stream`.
    #[snafu(display("Zerobus stream was closed concurrently"))]
    StreamClosed,
}

impl From<ZerobusError> for ZerobusSinkError {
    fn from(error: ZerobusError) -> Self {
        ZerobusSinkError::ZerobusError { source: error }
    }
}

/// Convert Zerobus errors to Vector event status.
impl From<ZerobusSinkError> for EventStatus {
    fn from(error: ZerobusSinkError) -> Self {
        match error {
            ZerobusSinkError::ConfigError { .. }
            | ZerobusSinkError::EncodingError { .. }
            | ZerobusSinkError::MissingAckOffset => EventStatus::Rejected,
            ZerobusSinkError::StreamClosed => EventStatus::Errored,
            ZerobusSinkError::ZerobusError { source }
            | ZerobusSinkError::StreamInitError { source }
            | ZerobusSinkError::IngestionError { source } => {
                if source.is_retryable() {
                    EventStatus::Errored
                } else {
                    EventStatus::Rejected
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::databricks_zerobus::service::ZerobusRetryLogic;
    use crate::sinks::util::retries::RetryLogic;

    fn retryable_error() -> ZerobusError {
        // ChannelCreationError is always retryable
        ZerobusError::ChannelCreationError("connection reset".to_string())
    }

    fn non_retryable_error() -> ZerobusError {
        // InvalidArgument is never retryable
        ZerobusError::InvalidArgument("bad field".to_string())
    }

    #[test]
    fn retryable_ingestion_error_maps_to_errored() {
        let error = ZerobusSinkError::IngestionError {
            source: retryable_error(),
        };
        assert_eq!(EventStatus::from(error), EventStatus::Errored);
    }

    #[test]
    fn non_retryable_ingestion_error_maps_to_rejected() {
        let error = ZerobusSinkError::IngestionError {
            source: non_retryable_error(),
        };
        assert_eq!(EventStatus::from(error), EventStatus::Rejected);
    }

    #[test]
    fn retryable_stream_init_error_maps_to_errored() {
        let error = ZerobusSinkError::StreamInitError {
            source: retryable_error(),
        };
        assert_eq!(EventStatus::from(error), EventStatus::Errored);
    }

    #[test]
    fn non_retryable_stream_init_error_maps_to_rejected() {
        let error = ZerobusSinkError::StreamInitError {
            source: non_retryable_error(),
        };
        assert_eq!(EventStatus::from(error), EventStatus::Rejected);
    }

    #[test]
    fn config_error_maps_to_rejected() {
        let error = ZerobusSinkError::ConfigError {
            message: "bad config".to_string(),
        };
        assert_eq!(EventStatus::from(error), EventStatus::Rejected);
    }

    #[test]
    fn encoding_error_maps_to_rejected() {
        let error = ZerobusSinkError::EncodingError {
            message: "encode failed".to_string(),
        };
        assert_eq!(EventStatus::from(error), EventStatus::Rejected);
    }

    #[test]
    fn retry_logic_retryable_errors() {
        let logic = ZerobusRetryLogic;

        let error = ZerobusSinkError::IngestionError {
            source: retryable_error(),
        };
        assert!(logic.is_retriable_error(&error));

        let error = ZerobusSinkError::StreamInitError {
            source: retryable_error(),
        };
        assert!(logic.is_retriable_error(&error));

        let error = ZerobusSinkError::ZerobusError {
            source: retryable_error(),
        };
        assert!(logic.is_retriable_error(&error));
    }

    #[test]
    fn retry_logic_non_retryable_errors() {
        let logic = ZerobusRetryLogic;

        let error = ZerobusSinkError::IngestionError {
            source: non_retryable_error(),
        };
        assert!(!logic.is_retriable_error(&error));

        let error = ZerobusSinkError::ConfigError {
            message: "bad".to_string(),
        };
        assert!(!logic.is_retriable_error(&error));

        let error = ZerobusSinkError::EncodingError {
            message: "bad".to_string(),
        };
        assert!(!logic.is_retriable_error(&error));
    }
}
