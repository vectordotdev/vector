use getset::{Getters, Setters};
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

pub static LOG_SCHEMA: OnceCell<LogSchema> = OnceCell::new();

lazy_static! {
    static ref LOG_SCHEMA_DEFAULT: LogSchema = LogSchema {
        message_key: Atom::from("message"),
        timestamp_key: Atom::from("timestamp"),
        host_key: Atom::from("host"),
        source_type_key: Atom::from("source_type"),
    };
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
            message_key: Atom::from("message"),
            timestamp_key: Atom::from("timestamp"),
            host_key: Atom::from("host"),
            source_type_key: Atom::from("source_type"),
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
}
