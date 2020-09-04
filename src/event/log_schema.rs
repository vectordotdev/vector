use getset::{Getters, Setters};
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

pub static LOG_SCHEMA: OnceCell<LogSchema> = OnceCell::new();

lazy_static! {
    static ref LOG_SCHEMA_DEFAULT: LogSchema = LogSchema::default();
}

pub fn log_schema() -> &'static LogSchema {
    LOG_SCHEMA.get().unwrap_or(&LOG_SCHEMA_DEFAULT)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Getters, Setters)]
#[serde(default)]
pub struct LogSchema {
    #[serde(default = "LogSchema::default_message_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    message_key: Atom,
    #[serde(default = "LogSchema::default_timestamp_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    timestamp_key: Atom,
    #[serde(default = "LogSchema::default_host_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    host_key: Atom,
    #[serde(default = "LogSchema::default_source_type_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    source_type_key: Atom,
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
    fn default_message_key() -> Atom {
        Atom::from("message")
    }

    fn default_timestamp_key() -> Atom {
        Atom::from("timestamp")
    }

    fn default_host_key() -> Atom {
        Atom::from("host")
    }

    fn default_source_type_key() -> Atom {
        Atom::from("source_type")
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
                self.set_host_key(other.host_key().clone());
            }
            if self.message_key() != LOG_SCHEMA_DEFAULT.message_key()
                && self.message_key() != other.message_key()
            {
                errors.push("conflicting values for 'log_schema.message_key' found".to_owned());
            } else {
                self.set_message_key(other.message_key().clone());
            }
            if self.timestamp_key() != LOG_SCHEMA_DEFAULT.timestamp_key()
                && self.timestamp_key() != other.timestamp_key()
            {
                errors.push("conflicting values for 'log_schema.timestamp_key' found".to_owned());
            } else {
                self.set_timestamp_key(other.timestamp_key().clone());
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
