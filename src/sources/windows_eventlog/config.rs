use std::{collections::HashMap, path::PathBuf};

use vector_config::component::GenerateConfig;
use vector_lib::configurable::configurable_component;


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

        // Enhanced security validation for poll intervals to prevent DoS
        if self.poll_interval_secs == 0 || self.poll_interval_secs > 3600 {
            return Err("Poll interval must be greater than 0".into());
        }

        // Prevent resource exhaustion via excessive batch sizes
        if self.batch_size == 0 || self.batch_size > 10000 {
            return Err("Batch size must be greater than 0".into());
        }

        // Validate read limits to prevent memory exhaustion
        if self.read_limit_bytes == 0 || self.read_limit_bytes > 100 * 1024 * 1024 {
            return Err("Read limit must be greater than 0".into());
        }

        // Enhanced channel name validation with security checks
        for channel in &self.channels {
            if channel.trim().is_empty() {
                return Err("Channel names cannot be empty".into());
            }
            
            // Prevent excessively long channel names
            if channel.len() > 256 {
                return Err(format!("Channel name '{}' exceeds maximum length of 256 characters", channel).into());
            }
            
            // Validate channel name contains only safe characters
            if !channel.chars().all(|c| c.is_ascii_alphanumeric() || "-_ /\\".contains(c)) {
                return Err(format!("Channel name '{}' contains invalid characters", channel).into());
            }
        }

        // Enhanced XPath query validation with injection protection
        if let Some(ref query) = self.event_query {
            if query.trim().is_empty() {
                return Err("Event query cannot be empty".into());
            }
            
            // Prevent excessively long XPath queries
            if query.len() > 4096 {
                return Err("Event query exceeds maximum length of 4096 characters".into());
            }
            
            // Check for unbalanced brackets and parentheses
            let mut bracket_count = 0;
            let mut paren_count = 0;
            
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
            let dangerous_patterns = [
                "javascript:", "vbscript:", "file:", "http:", "https:", "ftp:",
                "<script", "</script", "eval(", "expression(", "document.",
                "import ", "exec(", "system(", "cmd.exe", "powershell"
            ];
            
            let query_lower = query.to_lowercase();
            for pattern in &dangerous_patterns {
                if query_lower.contains(pattern) {
                    return Err(format!("Event query contains potentially unsafe pattern: '{}'", pattern).into());
                }
            }
            
            // Basic XPath syntax validation - check balanced brackets and parentheses
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
                
                // Prevent negative counts which indicate malformed syntax
                if bracket_count < 0 || paren_count < 0 {
                    return Err("Event query has malformed syntax - unbalanced brackets or parentheses".into());
                }
            }
            
            if bracket_count != 0 || paren_count != 0 {
                return Err("Event query has unbalanced brackets or parentheses".into());
            }
        }

        // Validate event ID filter lists to prevent resource exhaustion
        if let Some(ref event_ids) = self.only_event_ids {
            if event_ids.is_empty() {
                return Err("Only event IDs list cannot be empty when specified".into());
            }
            
            if event_ids.len() > 1000 {
                return Err("Only event IDs list cannot contain more than 1000 entries".into());
            }
        }
        
        if self.ignore_event_ids.len() > 1000 {
            return Err("Ignore event IDs list cannot contain more than 1000 entries".into());
        }

        // Validate field filter settings
        if let Some(ref include_fields) = self.field_filter.include_fields {
            if include_fields.is_empty() {
                return Err("Include fields list cannot be empty when specified".into());
            }
            
            if include_fields.len() > 100 {
                return Err("Include fields list cannot contain more than 100 entries".into());
            }
            
            for field in include_fields {
                if field.trim().is_empty() || field.len() > 128 {
                    return Err(format!("Invalid field name: '{}'", field).into());
                }
                
                // Enhanced security validation for field names
                if field.contains('\0') || field.contains('\r') || field.contains('\n') 
                   || field.contains('<') || field.contains('>') {
                    return Err(format!("Invalid field name contains dangerous characters: '{}'", field).into());
                }
            }
        }

        if let Some(ref exclude_fields) = self.field_filter.exclude_fields {
            if exclude_fields.is_empty() {
                return Err("Exclude fields list cannot be empty when specified".into());
            }
            
            if exclude_fields.len() > 100 {
                return Err("Exclude fields list cannot contain more than 100 entries".into());
            }
            
            for field in exclude_fields {
                if field.trim().is_empty() || field.len() > 128 {
                    return Err(format!("Invalid field name: '{}'", field).into());
                }
                
                // Enhanced security validation for field names
                if field.contains('\0') || field.contains('\r') || field.contains('\n') 
                   || field.contains('<') || field.contains('>') {
                    return Err(format!("Invalid field name contains dangerous characters: '{}'", field).into());
                }
            }
        }

        // Validate bookmark database path for path traversal attacks
        if let Some(ref db_path) = self.bookmark_db_path {
            let path_str = db_path.to_string_lossy();
            
            // Prevent path traversal attacks
            if path_str.contains("..") {
                return Err("Bookmark database path cannot contain '..' components".into());
            }
            
            // Prevent excessively long paths
            if path_str.len() > 512 {
                return Err("Bookmark database path is too long".into());
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
