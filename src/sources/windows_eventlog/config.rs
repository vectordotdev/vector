use std::{collections::HashMap, path::PathBuf};

use vector_config::component::GenerateConfig;
use vector_lib::configurable::configurable_component;

// Validation constants
const MAX_CHANNEL_NAME_LENGTH: usize = 256;
const MAX_XPATH_QUERY_LENGTH: usize = 4096;
const MAX_FIELD_NAME_LENGTH: usize = 128;
const MAX_FIELD_COUNT: usize = 100;
const MAX_EVENT_ID_LIST_SIZE: usize = 1000;
const MAX_CONNECTION_TIMEOUT_SECS: u64 = 3600;
const MAX_EVENT_TIMEOUT_MS: u64 = 60000;
const MAX_BATCH_SIZE: u32 = 10000;
const MAX_READ_LIMIT_BYTES: usize = 100 * 1024 * 1024; // 100 MB

/// Configuration for the `windows_eventlog` source.
#[configurable_component(source(
    "windows_eventlog",
    "Collect logs from Windows Event Log channels using the Windows Event Log API."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WindowsEventLogConfig {
    /// A comma-separated list of channels to read from.
    ///
    /// Common channels include "System", "Application", "Security", "Windows PowerShell".
    /// Use Windows Event Viewer to discover available channels.
    #[configurable(metadata(docs::examples = "System,Application,Security"))]
    #[configurable(metadata(docs::examples = "System"))]
    pub channels: Vec<String>,

    /// The XPath query for filtering events.
    ///
    /// Allows filtering events using XML Path Language queries.
    /// If not specified, all events from the specified channels will be collected.
    #[configurable(metadata(docs::examples = "*[System[Level=1 or Level=2 or Level=3]]"))]
    #[configurable(metadata(
        docs::examples = "*[System[(Level=1 or Level=2 or Level=3) and TimeCreated[timediff(@SystemTime) <= 86400000]]]"
    ))]
    pub event_query: Option<String>,

    /// Connection timeout in seconds for event subscription.
    ///
    /// This controls how long to wait for event subscription connection.
    #[serde(default = "default_connection_timeout_secs")]
    #[configurable(metadata(docs::examples = 30))]
    #[configurable(metadata(docs::examples = 60))]
    pub connection_timeout_secs: u64,

    /// Whether to read existing events or only new events.
    ///
    /// When set to `true`, the source will read all existing events from the channels.
    /// When set to `false` (default), only new events will be read.
    #[serde(default = "default_read_existing_events")]
    pub read_existing_events: bool,

    /// Batch size for event processing.
    ///
    /// This controls how many events are processed in a single batch.
    #[serde(default = "default_batch_size")]
    #[configurable(metadata(docs::examples = 10))]
    #[configurable(metadata(docs::examples = 100))]
    pub batch_size: u32,

    /// Maximum size in bytes to read per polling cycle.
    ///
    /// This controls the maximum amount of data read from Windows Event Logs
    /// in a single polling cycle to prevent memory issues.
    #[serde(default = "default_read_limit_bytes")]
    pub read_limit_bytes: usize,

    /// Whether to render the event message.
    ///
    /// When enabled, the source will attempt to render the full event message
    /// using the event's provider message file.
    #[serde(default = "default_render_message")]
    pub render_message: bool,

    /// Whether to include raw XML data in the output.
    ///
    /// When enabled, the raw XML representation of the event is included
    /// in the `xml` field of the output event.
    #[serde(default = "default_include_xml")]
    pub include_xml: bool,

    /// Custom event data formatting options.
    ///
    /// Maps event field names to custom formatting options.
    #[serde(default)]
    pub event_data_format: HashMap<String, EventDataFormat>,

    /// Ignore specific event IDs.
    ///
    /// Events with these IDs will be filtered out and not sent downstream.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "[4624, 4625, 4634]"))]
    pub ignore_event_ids: Vec<u32>,

    /// Only include specific event IDs.
    ///
    /// If specified, only events with these IDs will be processed.
    /// Takes precedence over `ignore_event_ids`.
    #[configurable(metadata(docs::examples = "[1000, 1001, 1002]"))]
    pub only_event_ids: Option<Vec<u32>>,

    /// Maximum age of events to process (in seconds).
    ///
    /// Events older than this value will be ignored. If not specified,
    /// all events will be processed regardless of age.
    #[configurable(metadata(docs::examples = 86400))]
    #[configurable(metadata(docs::examples = 604800))]
    pub max_event_age_secs: Option<u64>,

    /// Event delivery timeout in milliseconds.
    ///
    /// Maximum time to wait for event delivery before timing out.
    #[serde(default = "default_event_timeout_ms")]
    #[configurable(metadata(docs::examples = 5000))]
    #[configurable(metadata(docs::examples = 10000))]
    pub event_timeout_ms: u64,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    /// Event field inclusion/exclusion patterns.
    ///
    /// Controls which event fields are included in the output.
    #[serde(default)]
    pub field_filter: FieldFilter,

    /// The directory where checkpoint data is stored.
    ///
    /// Vector stores the last processed record ID for each channel to allow
    /// resuming from the correct position after restarts.
    #[serde(default = "default_data_dir")]
    #[configurable(metadata(docs::examples = "/var/lib/vector/windows_eventlog"))]
    #[configurable(metadata(docs::examples = "C:\\ProgramData\\vector\\windows_eventlog"))]
    pub data_dir: PathBuf,

    /// Maximum number of events to process per second.
    ///
    /// When set to a non-zero value, Vector will rate-limit event processing
    /// to prevent overwhelming downstream systems. A value of 0 (default) means
    /// no rate limiting is applied.
    #[serde(default = "default_events_per_second")]
    #[configurable(metadata(docs::examples = 100))]
    #[configurable(metadata(docs::examples = 1000))]
    #[configurable(metadata(docs::examples = 5000))]
    pub events_per_second: u32,

    /// Maximum length for event data field values.
    ///
    /// Event data values longer than this will be truncated with "...[truncated]" appended.
    /// Set to 0 for no limit (matches Winlogbeat behavior).
    #[serde(default = "default_max_event_data_length")]
    #[configurable(metadata(docs::examples = 1024))]
    #[configurable(metadata(docs::examples = 4096))]
    pub max_event_data_length: usize,

    /// Maximum length for message summary field.
    ///
    /// The message field contains a human-readable summary with event data samples.
    /// Individual event data values in the message will be truncated to this length.
    /// Set to 0 for no limit (matches Winlogbeat behavior).
    #[serde(default = "default_max_message_field_length")]
    #[configurable(metadata(docs::examples = 256))]
    #[configurable(metadata(docs::examples = 1024))]
    pub max_message_field_length: usize,
}

/// Event data formatting options.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum EventDataFormat {
    /// Format as string
    String,
    /// Format as integer
    Integer,
    /// Format as floating-point number
    Float,
    /// Format as boolean
    Boolean,
    /// Keep original format
    Auto,
}

/// Field filtering configuration.
#[configurable_component]
#[derive(Clone, Debug)]
pub struct FieldFilter {
    /// Fields to include in the output.
    ///
    /// If specified, only these fields will be included.
    pub include_fields: Option<Vec<String>>,

    /// Fields to exclude from the output.
    ///
    /// These fields will be removed from the event data.
    pub exclude_fields: Option<Vec<String>>,

    /// Whether to include system fields.
    ///
    /// System fields include metadata like Computer, TimeCreated, etc.
    #[serde(default = "default_include_system_fields")]
    pub include_system_fields: bool,

    /// Whether to include event data fields.
    ///
    /// Event data fields contain application-specific data.
    #[serde(default = "default_include_event_data")]
    pub include_event_data: bool,

    /// Whether to include user data fields.
    ///
    /// User data fields contain additional custom data.
    #[serde(default = "default_include_user_data")]
    pub include_user_data: bool,
}

impl Default for FieldFilter {
    fn default() -> Self {
        Self {
            include_fields: None,
            exclude_fields: None,
            include_system_fields: default_include_system_fields(),
            include_event_data: default_include_event_data(),
            include_user_data: default_include_user_data(),
        }
    }
}

impl Default for WindowsEventLogConfig {
    fn default() -> Self {
        Self {
            channels: vec!["System".to_string(), "Application".to_string()],
            event_query: None,
            connection_timeout_secs: default_connection_timeout_secs(),
            read_existing_events: default_read_existing_events(),
            batch_size: default_batch_size(),
            read_limit_bytes: default_read_limit_bytes(),
            render_message: default_render_message(),
            include_xml: default_include_xml(),
            event_data_format: HashMap::new(),
            ignore_event_ids: Vec::new(),
            only_event_ids: None,
            max_event_age_secs: None,
            event_timeout_ms: default_event_timeout_ms(),
            log_namespace: None,
            field_filter: FieldFilter::default(),
            data_dir: default_data_dir(),
            events_per_second: default_events_per_second(),
            max_event_data_length: default_max_event_data_length(),
            max_message_field_length: default_max_message_field_length(),
        }
    }
}

impl GenerateConfig for WindowsEventLogConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(WindowsEventLogConfig::default()).unwrap()
    }
}

impl WindowsEventLogConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), crate::Error> {
        if self.channels.is_empty() {
            return Err("At least one channel must be specified".into());
        }

        // Enhanced security validation for connection timeout to prevent DoS
        if self.connection_timeout_secs == 0
            || self.connection_timeout_secs > MAX_CONNECTION_TIMEOUT_SECS
        {
            return Err(format!(
                "Connection timeout must be between 1 and {} seconds",
                MAX_CONNECTION_TIMEOUT_SECS
            )
            .into());
        }

        // Validate event timeout
        if self.event_timeout_ms == 0 || self.event_timeout_ms > MAX_EVENT_TIMEOUT_MS {
            return Err(format!(
                "Event timeout must be between 1 and {} milliseconds",
                MAX_EVENT_TIMEOUT_MS
            )
            .into());
        }

        // Prevent resource exhaustion via excessive batch sizes
        if self.batch_size == 0 || self.batch_size > MAX_BATCH_SIZE {
            return Err(format!("Batch size must be between 1 and {}", MAX_BATCH_SIZE).into());
        }

        // Validate read limits to prevent memory exhaustion
        if self.read_limit_bytes == 0 || self.read_limit_bytes > MAX_READ_LIMIT_BYTES {
            return Err(format!(
                "Read limit must be between 1 and {} bytes",
                MAX_READ_LIMIT_BYTES
            )
            .into());
        }

        // Enhanced channel name validation with security checks
        for channel in &self.channels {
            if channel.trim().is_empty() {
                return Err("Channel names cannot be empty".into());
            }

            // Prevent excessively long channel names
            if channel.len() > MAX_CHANNEL_NAME_LENGTH {
                return Err(format!(
                    "Channel name '{}' exceeds maximum length of {} characters",
                    channel, MAX_CHANNEL_NAME_LENGTH
                )
                .into());
            }

            // Validate channel name contains only safe characters
            if !channel
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_ /\\".contains(c))
            {
                return Err(
                    format!("Channel name '{}' contains invalid characters", channel).into(),
                );
            }
        }

        // Enhanced XPath query validation with injection protection
        if let Some(ref query) = self.event_query {
            if query.trim().is_empty() {
                return Err("Event query cannot be empty".into());
            }

            // Prevent excessively long XPath queries
            if query.len() > MAX_XPATH_QUERY_LENGTH {
                return Err(format!(
                    "Event query exceeds maximum length of {} characters",
                    MAX_XPATH_QUERY_LENGTH
                )
                .into());
            }

            // Check for unbalanced brackets and parentheses
            let mut bracket_count = 0i32;
            let mut paren_count = 0i32;

            for ch in query.chars() {
                match ch {
                    '[' => bracket_count += 1,
                    ']' => bracket_count -= 1,
                    '(' => paren_count += 1,
                    ')' => paren_count -= 1,
                    _ => {}
                }

                // Check for negative counts (more closing than opening)
                if bracket_count < 0 || paren_count < 0 {
                    return Err("Event query contains unbalanced brackets or parentheses".into());
                }
            }

            // Check for unmatched opening brackets/parentheses
            if bracket_count != 0 || paren_count != 0 {
                return Err("Event query contains unbalanced brackets or parentheses".into());
            }

            // Check for potentially dangerous patterns that could indicate XPath injection
            // Note: We exclude "http:" and "https:" as they are legitimate in XML namespace URIs
            let dangerous_patterns = [
                "javascript:",
                "vbscript:",
                "file://", // Changed from "file:" to be more specific
                "ftp:",
                "<script",
                "</script",
                "eval(",
                "expression(",
                "document.",
                "import ",
                "exec(",
                "system(",
                "cmd.exe",
                "powershell",
            ];

            let query_lower = query.to_lowercase();
            for pattern in &dangerous_patterns {
                if query_lower.contains(pattern) {
                    return Err(format!(
                        "Event query contains potentially unsafe pattern: '{}'",
                        pattern
                    )
                    .into());
                }
            }
        }

        // Validate event ID filter lists to prevent resource exhaustion
        if let Some(ref event_ids) = self.only_event_ids {
            if event_ids.is_empty() {
                return Err("Only event IDs list cannot be empty when specified".into());
            }

            if event_ids.len() > MAX_EVENT_ID_LIST_SIZE {
                return Err(format!(
                    "Only event IDs list cannot contain more than {} entries",
                    MAX_EVENT_ID_LIST_SIZE
                )
                .into());
            }
        }

        if self.ignore_event_ids.len() > MAX_EVENT_ID_LIST_SIZE {
            return Err(format!(
                "Ignore event IDs list cannot contain more than {} entries",
                MAX_EVENT_ID_LIST_SIZE
            )
            .into());
        }

        // Validate field filter settings
        if let Some(ref include_fields) = self.field_filter.include_fields {
            if include_fields.is_empty() {
                return Err("Include fields list cannot be empty when specified".into());
            }

            if include_fields.len() > MAX_FIELD_COUNT {
                return Err(format!(
                    "Include fields list cannot contain more than {} entries",
                    MAX_FIELD_COUNT
                )
                .into());
            }

            for field in include_fields {
                if field.trim().is_empty() || field.len() > MAX_FIELD_NAME_LENGTH {
                    return Err(format!("Invalid field name: '{}'", field).into());
                }

                // Enhanced security validation for field names
                if field.contains('\0')
                    || field.contains('\r')
                    || field.contains('\n')
                    || field.contains('<')
                    || field.contains('>')
                {
                    return Err(format!(
                        "Invalid field name contains dangerous characters: '{}'",
                        field
                    )
                    .into());
                }
            }
        }

        if let Some(ref exclude_fields) = self.field_filter.exclude_fields {
            if exclude_fields.is_empty() {
                return Err("Exclude fields list cannot be empty when specified".into());
            }

            if exclude_fields.len() > MAX_FIELD_COUNT {
                return Err(format!(
                    "Exclude fields list cannot contain more than {} entries",
                    MAX_FIELD_COUNT
                )
                .into());
            }

            for field in exclude_fields {
                if field.trim().is_empty() || field.len() > MAX_FIELD_NAME_LENGTH {
                    return Err(format!("Invalid field name: '{}'", field).into());
                }

                // Enhanced security validation for field names
                if field.contains('\0')
                    || field.contains('\r')
                    || field.contains('\n')
                    || field.contains('<')
                    || field.contains('>')
                {
                    return Err(format!(
                        "Invalid field name contains dangerous characters: '{}'",
                        field
                    )
                    .into());
                }
            }
        }

        Ok(())
    }
}

// Default value functions
const fn default_connection_timeout_secs() -> u64 {
    30
}

const fn default_event_timeout_ms() -> u64 {
    5000
}

const fn default_read_existing_events() -> bool {
    false
}

const fn default_batch_size() -> u32 {
    10
}

const fn default_read_limit_bytes() -> usize {
    524_288 // 512 KiB
}

const fn default_render_message() -> bool {
    true
}

const fn default_include_xml() -> bool {
    false
}

const fn default_include_system_fields() -> bool {
    true
}

const fn default_include_event_data() -> bool {
    true
}

const fn default_include_user_data() -> bool {
    true
}

fn default_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from("C:\\ProgramData\\vector\\windows_eventlog")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/var/lib/vector/windows_eventlog")
    }
}

const fn default_events_per_second() -> u32 {
    0 // 0 means no rate limiting
}

const fn default_max_event_data_length() -> usize {
    0 // 0 means no truncation (matches Winlogbeat)
}

const fn default_max_message_field_length() -> usize {
    0 // 0 means no truncation (matches Winlogbeat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WindowsEventLogConfig::default();
        assert_eq!(config.channels, vec!["System", "Application"]);
        assert_eq!(config.connection_timeout_secs, 30);
        assert_eq!(config.event_timeout_ms, 5000);
        assert!(!config.read_existing_events);
        assert_eq!(config.batch_size, 10);
        assert!(config.render_message);
        assert!(!config.include_xml);
    }

    #[test]
    fn test_config_validation() {
        let mut config = WindowsEventLogConfig::default();

        // Valid configuration should pass
        assert!(config.validate().is_ok());

        // Empty channels should fail
        config.channels = vec![];
        assert!(config.validate().is_err());

        // Reset channels
        config.channels = vec!["System".to_string()];
        assert!(config.validate().is_ok());

        // Zero connection timeout should fail
        config.connection_timeout_secs = 0;
        assert!(config.validate().is_err());

        // Reset connection timeout
        config.connection_timeout_secs = 30;
        assert!(config.validate().is_ok());

        // Zero batch size should fail
        config.batch_size = 0;
        assert!(config.validate().is_err());

        // Reset batch size
        config.batch_size = 10;
        assert!(config.validate().is_ok());

        // Zero read limit should fail
        config.read_limit_bytes = 0;
        assert!(config.validate().is_err());

        // Empty channel name should fail
        config.channels = vec!["".to_string()];
        config.read_limit_bytes = default_read_limit_bytes();
        assert!(config.validate().is_err());

        // Empty query should fail
        config.channels = vec!["System".to_string()];
        config.event_query = Some("".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_field_filter_default() {
        let filter = FieldFilter::default();
        assert!(filter.include_system_fields);
        assert!(filter.include_event_data);
        assert!(filter.include_user_data);
        assert!(filter.include_fields.is_none());
        assert!(filter.exclude_fields.is_none());
    }

    #[test]
    fn test_serialization() {
        let config = WindowsEventLogConfig {
            channels: vec!["System".to_string(), "Application".to_string()],
            event_query: Some("*[System[Level=1]]".to_string()),
            connection_timeout_secs: 30,
            read_existing_events: true,
            batch_size: 50,
            read_limit_bytes: 1024000,
            render_message: false,
            include_xml: true,
            event_data_format: HashMap::new(),
            ignore_event_ids: vec![4624, 4625],
            only_event_ids: Some(vec![1000, 1001]),
            max_event_age_secs: Some(86400),
            event_timeout_ms: 5000,
            log_namespace: Some(true),
            field_filter: FieldFilter::default(),
            data_dir: PathBuf::from("/test/data"),
            events_per_second: 1000,
        };

        // Should serialize and deserialize without errors
        let serialized = serde_json::to_string(&config).expect("serialization should succeed");
        let deserialized: WindowsEventLogConfig =
            serde_json::from_str(&serialized).expect("deserialization should succeed");

        assert_eq!(config.channels, deserialized.channels);
        assert_eq!(config.event_query, deserialized.event_query);
        assert_eq!(
            config.connection_timeout_secs,
            deserialized.connection_timeout_secs
        );
        assert_eq!(
            config.read_existing_events,
            deserialized.read_existing_events
        );
        assert_eq!(config.batch_size, deserialized.batch_size);
    }
}
