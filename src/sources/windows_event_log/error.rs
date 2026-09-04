use snafu::Snafu;

/// Errors that can occur when working with Windows Event Logs.
#[derive(Debug, Snafu)]
pub enum WindowsEventLogError {
    #[snafu(display("Failed to open event log channel '{}': {}", channel, source))]
    OpenChannelError {
        channel: String,
        source: windows::core::Error,
    },

    #[snafu(display("Failed to create event subscription: {}", source))]
    CreateSubscriptionError { source: windows::core::Error },

    #[snafu(display("Failed to query events: {}", source))]
    QueryEventsError { source: windows::core::Error },

    #[snafu(display("Failed to read event: {}", source))]
    ReadEventError { source: windows::core::Error },

    #[snafu(display("Failed to render event message: {}", source))]
    RenderMessageError { source: windows::core::Error },

    #[snafu(display("Failed to parse event XML: {}", source))]
    ParseXmlError { source: quick_xml::Error },

    #[snafu(display("Invalid XPath query '{}': {}", query, message))]
    InvalidXPathQuery { query: String, message: String },

    #[snafu(display(
        "Access denied to channel '{}'. Administrator privileges may be required",
        channel
    ))]
    AccessDeniedError { channel: String },

    #[snafu(display("Channel '{}' not found", channel))]
    ChannelNotFoundError { channel: String },

    #[snafu(display("I/O error: {}", source))]
    IoError { source: std::io::Error },

    #[snafu(display("Event filtering error: {}", message))]
    FilterError { message: String },

    #[snafu(display("Configuration error: {}", message))]
    ConfigError { message: String },

    #[snafu(display("System resource exhausted: {}", message))]
    ResourceExhaustedError { message: String },

    #[snafu(display("Operation timeout after {} seconds", timeout_secs))]
    TimeoutError { timeout_secs: u64 },

    #[snafu(display("Failed to create render context: {}", source))]
    CreateRenderContextError { source: windows::core::Error },

    #[snafu(display("Failed to format message: {}", message))]
    FormatMessageError { message: String },

    #[snafu(display("Failed to render event: {}", message))]
    RenderError { message: String },

    #[snafu(display("Failed to create subscription: {}", source))]
    SubscriptionError { source: windows::core::Error },

    #[snafu(display("Failed to seek events: {}", source))]
    SeekEventsError { source: windows::core::Error },

    #[snafu(display("Failed to load publisher metadata for '{}': {}", provider, source))]
    LoadPublisherMetadataError {
        provider: String,
        source: windows::core::Error,
    },

    #[snafu(display("Failed to pull events from channel '{}': {}", channel, source))]
    PullEventsError {
        channel: String,
        source: windows::core::Error,
    },
}

impl WindowsEventLogError {
    /// Check if the error is recoverable and the operation should be retried.
    pub const fn is_recoverable(&self) -> bool {
        match self {
            // Network/connection issues are typically recoverable
            Self::QueryEventsError { .. }
            | Self::ReadEventError { .. }
            | Self::ResourceExhaustedError { .. }
            | Self::TimeoutError { .. }
            | Self::SeekEventsError { .. }
            | Self::PullEventsError { .. } => true,

            // Configuration and permission issues are not recoverable
            Self::OpenChannelError { .. }
            | Self::CreateSubscriptionError { .. }
            | Self::SubscriptionError { .. }
            | Self::AccessDeniedError { .. }
            | Self::ChannelNotFoundError { .. }
            | Self::InvalidXPathQuery { .. }
            | Self::ConfigError { .. }
            | Self::CreateRenderContextError { .. }
            | Self::LoadPublisherMetadataError { .. } => false,

            // Parsing errors might be recoverable depending on the specific error
            Self::ParseXmlError { .. }
            | Self::RenderMessageError { .. }
            | Self::FormatMessageError { .. }
            | Self::RenderError { .. } => false,

            // I/O errors could be temporary
            Self::IoError { .. } => true,

            Self::FilterError { .. } => false,
        }
    }

    /// Get a user-friendly error message for logging.
    pub fn user_message(&self) -> String {
        match self {
            Self::AccessDeniedError { channel } => {
                format!(
                    "Access denied to event log channel '{}'. Try running Vector as Administrator.",
                    channel
                )
            }
            Self::ChannelNotFoundError { channel } => {
                format!(
                    "Event log channel '{}' not found. Check the channel name and ensure the service is installed.",
                    channel
                )
            }
            Self::InvalidXPathQuery { query, .. } => {
                format!("Invalid XPath query '{}'. Check the query syntax.", query)
            }
            Self::ResourceExhaustedError { .. } => {
                "System resources exhausted. Consider reducing batch_size or poll_interval_secs."
                    .to_string()
            }
            Self::TimeoutError { timeout_secs } => {
                format!(
                    "Operation timed out after {} seconds. Consider increasing timeout values.",
                    timeout_secs
                )
            }
            _ => self.to_string(),
        }
    }
}

impl From<quick_xml::Error> for WindowsEventLogError {
    fn from(error: quick_xml::Error) -> Self {
        Self::ParseXmlError { source: error }
    }
}

// Bookmark persistence is handled via the checkpoint module (JSON-based)

impl From<std::io::Error> for WindowsEventLogError {
    fn from(error: std::io::Error) -> Self {
        Self::IoError { source: error }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_recoverability() {
        let recoverable_errors = vec![
            WindowsEventLogError::ResourceExhaustedError {
                message: "test".to_string(),
            },
            WindowsEventLogError::TimeoutError { timeout_secs: 30 },
            WindowsEventLogError::IoError {
                source: std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"),
            },
        ];

        for error in recoverable_errors {
            assert!(
                error.is_recoverable(),
                "Error should be recoverable: {}",
                error
            );
        }

        let non_recoverable_errors = vec![
            WindowsEventLogError::AccessDeniedError {
                channel: "Security".to_string(),
            },
            WindowsEventLogError::ChannelNotFoundError {
                channel: "NonExistent".to_string(),
            },
            WindowsEventLogError::InvalidXPathQuery {
                query: "invalid".to_string(),
                message: "syntax error".to_string(),
            },
            WindowsEventLogError::ConfigError {
                message: "invalid config".to_string(),
            },
        ];

        for error in non_recoverable_errors {
            assert!(
                !error.is_recoverable(),
                "Error should not be recoverable: {}",
                error
            );
        }
    }

    #[test]
    fn test_user_messages() {
        let error = WindowsEventLogError::AccessDeniedError {
            channel: "Security".to_string(),
        };
        assert!(error.user_message().contains("Administrator"));

        let error = WindowsEventLogError::ChannelNotFoundError {
            channel: "NonExistent".to_string(),
        };
        assert!(error.user_message().contains("not found"));

        let error = WindowsEventLogError::InvalidXPathQuery {
            query: "*[invalid]".to_string(),
            message: "syntax error".to_string(),
        };
        assert!(error.user_message().contains("XPath query"));

        let error = WindowsEventLogError::TimeoutError { timeout_secs: 30 };
        assert!(error.user_message().contains("timed out"));
    }

    #[test]
    fn test_error_conversions() {
        let xml_error = quick_xml::Error::UnexpectedEof("test".to_string());
        let converted: WindowsEventLogError = xml_error.into();
        assert!(matches!(
            converted,
            WindowsEventLogError::ParseXmlError { .. }
        ));

        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test");
        let converted: WindowsEventLogError = io_error.into();
        assert!(matches!(converted, WindowsEventLogError::IoError { .. }));
    }
}
