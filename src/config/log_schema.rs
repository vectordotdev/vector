use getset::{Getters, Setters};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom;

pub static LOG_SCHEMA: OnceCell<LogSchema> = OnceCell::new();

lazy_static::lazy_static! {
    static ref LOG_SCHEMA_DEFAULT: LogSchema = LogSchema {
        message_key: DefaultAtom::from("message"),
        timestamp_key: DefaultAtom::from("timestamp"),
        host_key: DefaultAtom::from("host"),
        source_type_key: DefaultAtom::from("source_type"),
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
    message_key: DefaultAtom,
    #[serde(default = "LogSchema::default_timestamp_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    timestamp_key: DefaultAtom,
    #[serde(default = "LogSchema::default_host_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    host_key: DefaultAtom,
    #[serde(default = "LogSchema::default_source_type_key")]
    #[getset(get = "pub", set = "pub(crate)")]
    source_type_key: DefaultAtom,
}

impl Default for LogSchema {
    fn default() -> Self {
        LogSchema {
            message_key: DefaultAtom::from("message"),
            timestamp_key: DefaultAtom::from("timestamp"),
            host_key: DefaultAtom::from("host"),
            source_type_key: DefaultAtom::from("source_type"),
        }
    }
}

impl LogSchema {
    fn default_message_key() -> DefaultAtom {
        DefaultAtom::from("message")
    }
    fn default_timestamp_key() -> DefaultAtom {
        DefaultAtom::from("timestamp")
    }
    fn default_host_key() -> DefaultAtom {
        DefaultAtom::from("host")
    }
    fn default_source_type_key() -> DefaultAtom {
        DefaultAtom::from("source_type")
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
