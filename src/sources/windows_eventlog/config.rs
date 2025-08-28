use std::{collections::HashMap, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};
use vector_lib::config::LegacyKey;
use vector_lib::configurable::configurable_component;

use super::BuildError;

/// Configuration for the `windows_eventlog` source.
#[configurable_component(source(
    "windows_eventlog",
    "Collect logs from Windows Event Log channels using the Windows Event Log API."
))]
#[derive(Clone, Debug, Deserialize, Serialize)]
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

    /// Polling interval in seconds for reading events.
    ///
    /// This controls how frequently the source checks for new events.
    #[serde(default = "default_poll_interval_secs")]
    #[configurable(metadata(docs::examples = 1))]
    #[configurable(metadata(docs::examples = 30))]
    pub poll_interval_secs: u64,

    /// Whether to read existing events or only new events.
    ///
    /// When set to `true`, the source will read all existing events from the channels.
    /// When set to `false` (default), only new events will be read.
    #[serde(default = "default_read_existing_events")]
    pub read_existing_events: bool,

    /// Path to the database file for storing bookmarks.
    ///
    /// This SQLite database is used to store the position of the last read event
    /// to avoid duplicating events on restart.
    #[configurable(metadata(docs::examples = "C:\\ProgramData\\vector\\winevtlog.db"))]
    #[configurable(metadata(docs::examples = "./winevtlog.db"))]
    pub bookmark_db_path: Option<PathBuf>,

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

    /// Whether to use real-time event subscription.
    ///
    /// When enabled, the source will use Windows Event Log subscription
    /// for real-time event delivery instead of polling.
    #[serde(default = "default_use_subscription")]
    pub use_subscription: bool,

    /// The namespace to use for logs. This overrides the global setting.
    #[configurable(metadata(docs::hidden))]
    #[serde(default)]
    pub log_namespace: Option<bool>,

    /// Event field inclusion/exclusion patterns.
    ///
    /// Controls which event fields are included in the output.
    #[serde(default)]
    pub field_filter: FieldFilter,
}

/// Event data formatting options.
#[configurable_component]
#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
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

impl Default for WindowsEventLogConfig {
    fn default() -> Self {
        Self {
            channels: vec!["System".to_string(), "Application".to_string()],
            event_query: None,
            poll_interval_secs: default_poll_interval_secs(),
            read_existing_events: default_read_existing_events(),
            bookmark_db_path: None,
            batch_size: default_batch_size(),
            read_limit_bytes: default_read_limit_bytes(),
            render_message: default_render_message(),
            include_xml: default_include_xml(),
            event_data_format: HashMap::new(),
            ignore_event_ids: Vec::new(),
            only_event_ids: None,
            max_event_age_secs: None,
            use_subscription: default_use_subscription(),
            log_namespace: None,
            field_filter: FieldFilter::default(),
        }
    }
}

impl WindowsEventLogConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), crate::Error> {
        if self.channels.is_empty() {
            return Err(Box::new(BuildError::InvalidConfiguration {
                message: "At least one channel must be specified".to_string(),
            }));
        }

        if self.poll_interval_secs == 0 {
            return Err(Box::new(BuildError::InvalidConfiguration {
                message: "Poll interval must be greater than 0".to_string(),
            }));
        }

        if self.batch_size == 0 {
            return Err(Box::new(BuildError::InvalidConfiguration {
                message: "Batch size must be greater than 0".to_string(),
            }));
        }

        if self.read_limit_bytes == 0 {
            return Err(Box::new(BuildError::InvalidConfiguration {
                message: "Read limit must be greater than 0".to_string(),
            }));
        }

        // Validate channels contain valid Windows Event Log channel names
        for channel in &self.channels {
            if channel.trim().is_empty() {
                return Err(Box::new(BuildError::InvalidConfiguration {
                    message: "Channel names cannot be empty".to_string(),
                }));
            }
        }

        // Validate XPath query syntax if provided
        if let Some(ref query) = self.event_query {
            if query.trim().is_empty() {
                return Err(Box::new(BuildError::InvalidConfiguration {
                    message: "Event query cannot be empty".to_string(),
                }));
            }
        }

        Ok(())
    }
}

// Default value functions
const fn default_poll_interval_secs() -> u64 {
    1
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

const fn default_use_subscription() -> bool {
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WindowsEventLogConfig::default();
        assert_eq!(config.channels, vec!["System", "Application"]);
        assert_eq!(config.poll_interval_secs, 1);
        assert!(!config.read_existing_events);
        assert_eq!(config.batch_size, 10);
        assert!(config.render_message);
        assert!(!config.include_xml);
        assert!(config.use_subscription);
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

        // Zero poll interval should fail
        config.poll_interval_secs = 0;
        assert!(config.validate().is_err());

        // Reset poll interval
        config.poll_interval_secs = 1;
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
            poll_interval_secs: 30,
            read_existing_events: true,
            bookmark_db_path: Some(PathBuf::from("test.db")),
            batch_size: 50,
            read_limit_bytes: 1024000,
            render_message: false,
            include_xml: true,
            event_data_format: HashMap::new(),
            ignore_event_ids: vec![4624, 4625],
            only_event_ids: Some(vec![1000, 1001]),
            max_event_age_secs: Some(86400),
            use_subscription: false,
            log_namespace: Some(true),
            field_filter: FieldFilter::default(),
        };

        // Should serialize and deserialize without errors
        let serialized = serde_json::to_string(&config).expect("serialization should succeed");
        let deserialized: WindowsEventLogConfig =
            serde_json::from_str(&serialized).expect("deserialization should succeed");

        assert_eq!(config.channels, deserialized.channels);
        assert_eq!(config.event_query, deserialized.event_query);
        assert_eq!(config.poll_interval_secs, deserialized.poll_interval_secs);
        assert_eq!(
            config.read_existing_events,
            deserialized.read_existing_events
        );
        assert_eq!(config.batch_size, deserialized.batch_size);
    }
}
