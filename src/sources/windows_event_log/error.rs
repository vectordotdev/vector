use snafu::Snafu;

/// Errors that can occur when working with Windows Event Logs.
#[derive(Debug, Snafu)]
pub enum WindowsEventLogError {
    #[snafu(display("Failed to open event log channel '{}': {}", channel, source))]
    OpenChannelError {
        channel: String,
        #[cfg(windows)]
        source: windows::core::Error,
        #[cfg(not(windows))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[snafu(display("Failed to create event subscription: {}", source))]
    CreateSubscriptionError {
        #[cfg(windows)]
        source: windows::core::Error,
        #[cfg(not(windows))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[snafu(display("Failed to query events: {}", source))]
    QueryEventsError {
        #[cfg(windows)]
        source: windows::core::Error,
        #[cfg(not(windows))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[snafu(display("Failed to read event: {}", source))]
    ReadEventError {
        #[cfg(windows)]
        source: windows::core::Error,
        #[cfg(not(windows))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[snafu(display("Failed to render event message: {}", source))]
    RenderMessageError {
        #[cfg(windows)]
        source: windows::core::Error,
        #[cfg(not(windows))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

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
    CreateRenderContextError {
        #[cfg(windows)]
        source: windows::core::Error,
        #[cfg(not(windows))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[snafu(display("Failed to format message: {}", message))]
    FormatMessageError { message: String },

    #[snafu(display("Failed to render event: {}", message))]
    RenderError { message: String },

    #[snafu(display("Failed to create subscription: {}", source))]
    SubscriptionError {
        #[cfg(windows)]
        source: windows::core::Error,
        #[cfg(not(windows))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[snafu(display("Failed to seek events: {}", source))]
    SeekEventsError {
        #[cfg(windows)]
        source: windows::core::Error,
        #[cfg(not(windows))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[snafu(display("Failed to load publisher metadata for '{}': {}", provider, source))]
    LoadPublisherMetadataError {
        provider: String,
        #[cfg(windows)]
        source: windows::core::Error,
        #[cfg(not(windows))]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[cfg(not(windows))]
    #[snafu(display("Windows Event Log functionality is only available on Windows"))]
    NotSupportedError,
}

impl WindowsEventLogError {
    /// Check if the error is recoverable and the operation should be retried.
    pub fn is_recoverable(&self) -> bool {
        match self {
            // Network/connection issues are typically recoverable
            Self::QueryEventsError { .. }
            | Self::ReadEventError { .. }
            | Self::ResourceExhaustedError { .. }
            | Self::TimeoutError { .. }
            | Self::SeekEventsError { .. } => true,

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

            #[cfg(not(windows))]
            Self::NotSupportedError => false,
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
            #[cfg(not(windows))]
            Self::NotSupportedError => {
                "Windows Event Log source is only supported on Windows operating systems."
                    .to_string()
            }
            _ => self.to_string(),
        }
    }
}

// Implement conversion from Windows errors
#[cfg(windows)]
impl From<windows::core::Error> for WindowsEventLogError {
    fn from(error: windows::core::Error) -> Self {
        match error.code().0 as u32 {
            5 => Self::AccessDeniedError {
                channel: "unknown".to_string(),
            },
            15007 => Self::ChannelNotFoundError {
                channel: "unknown".to_string(),
            },
            _ => Self::QueryEventsError { source: error },
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

    #[cfg(not(windows))]
    #[test]
    fn test_not_supported_error() {
        let error = WindowsEventLogError::NotSupportedError;
        assert!(!error.is_recoverable());
        assert!(error.user_message().contains("Windows operating systems"));
    }
}
