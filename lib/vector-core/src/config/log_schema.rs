use getset::{Getters, Setters};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

static LOG_SCHEMA: OnceCell<LogSchema> = OnceCell::new();

lazy_static::lazy_static! {
    static ref LOG_SCHEMA_DEFAULT: LogSchema = LogSchema::default();
}

/// Loads Log Schema from configurations and sets global schema. Once this is
/// done, configurations can be correctly loaded using configured log schema
/// defaults.
///
/// # Errors
///
/// This function will fail if the `builder` fails.
///
/// # Panics
///
/// If deny is set, will panic if schema has already been set.
pub fn init_log_schema<F>(builder: F, deny_if_set: bool) -> Result<(), Vec<String>>
where
    F: FnOnce() -> Result<LogSchema, Vec<String>>,
{
    let log_schema = builder()?;
    assert!(
        !(LOG_SCHEMA.set(log_schema).is_err() && deny_if_set),
        "Couldn't set schema"
    );

    Ok(())
}

/// Components should use global `LogSchema` returned by this function.  The
/// returned value can differ from `LogSchema::default()` which is unchanging.
pub fn log_schema() -> &'static LogSchema {
    LOG_SCHEMA.get().unwrap_or(&LOG_SCHEMA_DEFAULT)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Getters, Setters)]
#[serde(default)]
pub struct LogSchema {
    #[serde(default = "LogSchema::default_message_key")]
    message_key: String,
    #[serde(default = "LogSchema::default_timestamp_key")]
    timestamp_key: String,
    #[serde(default = "LogSchema::default_host_key")]
    host_key: String,
    #[serde(default = "LogSchema::default_source_type_key")]
    source_type_key: String,
    #[serde(default = "LogSchema::default_metadata_key")]
    metadata_key: String,
}

impl Default for LogSchema {
    fn default() -> Self {
        LogSchema {
            message_key: Self::default_message_key(),
            timestamp_key: Self::default_timestamp_key(),
            host_key: Self::default_host_key(),
            source_type_key: Self::default_source_type_key(),
            metadata_key: Self::default_metadata_key(),
        }
    }
}

impl LogSchema {
    fn default_message_key() -> String {
        String::from("message")
    }

    fn default_timestamp_key() -> String {
        String::from("timestamp")
    }

    fn default_host_key() -> String {
        String::from("host")
    }

    fn default_source_type_key() -> String {
        String::from("source_type")
    }

    fn default_metadata_key() -> String {
        String::from("metadata")
    }

    pub fn message_key(&self) -> &str {
        &self.message_key
    }

    pub fn timestamp_key(&self) -> &str {
        &self.timestamp_key
    }

    pub fn host_key(&self) -> &str {
        &self.host_key
    }

    pub fn source_type_key(&self) -> &str {
        &self.source_type_key
    }

    pub fn metadata_key(&self) -> &str {
        &self.metadata_key
    }

    pub fn set_message_key(&mut self, v: String) {
        self.message_key = v;
    }

    pub fn set_timestamp_key(&mut self, v: String) {
        self.timestamp_key = v;
    }

    pub fn set_host_key(&mut self, v: String) {
        self.host_key = v;
    }

    pub fn set_source_type_key(&mut self, v: String) {
        self.source_type_key = v;
    }

    pub fn set_metadata_key(&mut self, v: String) {
        self.metadata_key = v;
    }

    /// Merge two `LogSchema` instances together.
    ///
    /// # Errors
    ///
    /// This function will fail when the `LogSchema` to be merged contains
    /// conflicting keys.
    pub fn merge(&mut self, other: &LogSchema) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if *other != *LOG_SCHEMA_DEFAULT {
            // If the set value is the default, override it. If it's already overridden, error.
            if self.host_key() != LOG_SCHEMA_DEFAULT.host_key()
                && self.host_key() != other.host_key()
            {
                errors.push("conflicting values for 'log_schema.host_key' found".to_owned());
            } else {
                self.set_host_key(other.host_key().to_string());
            }
            if self.message_key() != LOG_SCHEMA_DEFAULT.message_key()
                && self.message_key() != other.message_key()
            {
                errors.push("conflicting values for 'log_schema.message_key' found".to_owned());
            } else {
                self.set_message_key(other.message_key().to_string());
            }
            if self.timestamp_key() != LOG_SCHEMA_DEFAULT.timestamp_key()
                && self.timestamp_key() != other.timestamp_key()
            {
                errors.push("conflicting values for 'log_schema.timestamp_key' found".to_owned());
            } else {
                self.set_timestamp_key(other.timestamp_key().to_string());
            }
            if self.source_type_key() != LOG_SCHEMA_DEFAULT.source_type_key()
                && self.source_type_key() != other.source_type_key()
            {
                errors.push("conflicting values for 'log_schema.source_type_key' found".to_owned());
            } else {
                self.set_source_type_key(other.source_type_key().to_string());
            }
            if self.metadata_key() != LOG_SCHEMA_DEFAULT.metadata_key()
                && self.metadata_key() != other.metadata_key()
            {
                errors.push("conflicting values for 'log_schema.metadata_key' found".to_owned());
            } else {
                self.set_metadata_key(other.metadata_key().to_string());
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn partial_log_schema() {
        let toml = r#"
            message_key = "message"
            timestamp_key = "timestamp"
        "#;
        toml::from_str::<LogSchema>(toml).unwrap();
    }
}
