use crate::event::{Lookup, LookupBuf};
use getset::{Getters, Setters};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
pub static LOG_SCHEMA: OnceCell<LogSchema> = OnceCell::new();

lazy_static::lazy_static! {
    static ref LOG_SCHEMA_DEFAULT: LogSchema = LogSchema {
        message_key: LookupBuf::from("message"),
        timestamp_key: LookupBuf::from("timestamp"),
        host_key: LookupBuf::from("host"),
        source_type_key: LookupBuf::from("source_type"),
    };
}
pub fn log_schema() -> &'static LogSchema {
    LOG_SCHEMA.get().unwrap_or(&LOG_SCHEMA_DEFAULT)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Getters, Setters)]
#[serde(default)]
pub struct LogSchema {
    #[serde(default = "LogSchema::default_message_key")]
    message_key: LookupBuf,
    #[serde(default = "LogSchema::default_timestamp_key")]
    timestamp_key: LookupBuf,
    #[serde(default = "LogSchema::default_host_key")]
    host_key: LookupBuf,
    #[serde(default = "LogSchema::default_source_type_key")]
    source_type_key: LookupBuf,
}

impl Default for LogSchema {
    fn default() -> Self {
        LogSchema {
            message_key: Self::default_message_key(),
            timestamp_key: Self::default_timestamp_key(),
            host_key: Self::default_host_key(),
            source_type_key: Self::default_source_type_key(),
        }
    }
}

impl LogSchema {
    fn default_message_key() -> LookupBuf {
        LookupBuf::from("message")
    }
    fn default_timestamp_key() -> LookupBuf {
        LookupBuf::from("timestamp")
    }
    fn default_host_key() -> LookupBuf {
        LookupBuf::from("host")
    }
    fn default_source_type_key() -> LookupBuf {
        LookupBuf::from("source_type")
    }

    pub fn message_key(&self) -> Lookup {
        self.message_key.as_lookup()
    }
    pub fn timestamp_key(&self) -> Lookup {
        self.timestamp_key.as_lookup()
    }
    pub fn host_key(&self) -> Lookup {
        self.host_key.as_lookup()
    }
    pub fn source_type_key(&self) -> Lookup {
        self.source_type_key.as_lookup()
    }

    pub fn set_message_key(&mut self, v: LookupBuf) {
        self.message_key = v.into();
    }
    pub fn set_timestamp_key(&mut self, v: LookupBuf) {
        self.timestamp_key = v.into();
    }
    pub fn set_host_key(&mut self, v: LookupBuf) {
        self.host_key = v.into();
    }
    pub fn set_source_type_key(&mut self, v: LookupBuf) {
        self.source_type_key = v.into();
    }

    pub fn merge(&mut self, other: LogSchema) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if other != *LOG_SCHEMA_DEFAULT {
            // If the set value is the default, override it. If it's already overridden, error.
            if self.host_key() != LOG_SCHEMA_DEFAULT.host_key()
                && self.host_key() != other.host_key()
            {
                errors.push("conflicting values for 'log_schema.host_key' found".to_owned());
            } else {
                self.set_host_key(other.host_key().into_buf());
            }
            if self.message_key() != LOG_SCHEMA_DEFAULT.message_key()
                && self.message_key() != other.message_key()
            {
                errors.push("conflicting values for 'log_schema.message_key' found".to_owned());
            } else {
                self.set_message_key(other.message_key().into_buf());
            }
            if self.timestamp_key() != LOG_SCHEMA_DEFAULT.timestamp_key()
                && self.timestamp_key() != other.timestamp_key()
            {
                errors.push("conflicting values for 'log_schema.timestamp_key' found".to_owned());
            } else {
                self.set_timestamp_key(other.timestamp_key().into_buf());
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
        let _ = toml::from_str::<LogSchema>(toml).unwrap();
    }
}
