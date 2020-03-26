//! Encoding related code.
//!
//! You'll find two stuctures for configuration:
//!   * `EncodingConfig<E>`: For sinks without a default `Encoding`.
//!   * `EncodingConfigWithDefault<E: Default>`: For sinks that have a default `Encoding`.
//!
//! Your sink should define some `Encoding` enum that is used as the `E` parameter.
//!
//! You can use either of these for a sink! They both implement `EncodingConfiguration`, which you
//! will need to import as well.
//!
//! # Using a configuration
//!
//! To use an `EncodingConfig` involves three steps:
//!
//!  1. Choose between `EncodingConfig` and `EncodingConfigWithDefault`.
//!  2. Call `apply_rules(&mut event)` on this config **on each event** just before it gets sent.
//!
//! # Implementation notes
//!
//! You may wonder why we have both of these types! **Great question.** `serde` works with the
//! static `*SinkConfig` types when it deserializes our configuration. This means `serde` needs to
//! statically be aware if there is a default for some given `E` of the config. Since
//! We don't require `E: Default` we can't always assume that, so we need to create statically
//! distinct types! Having `EncodingConfigWithDefault` is a relatively straightforward way to
//! accomplish this without a bunch of magic.
//!
// TODO: To avoid users forgetting to apply the rules, the `E` param should require a trait
//       `Encoder` that defines some `encode` function which this config then calls internally as
//       part of it's own (yet to be written) `encode() -> Vec<u8>` function.

mod encoding_config;
pub use encoding_config::EncodingConfig;
mod encoding_config_with_default;
pub use encoding_config_with_default::EncodingConfigWithDefault;
mod inner;
mod inner_with_default;
use inner_with_default::InnerWithDefault;

use crate::{event::Value, Event, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt::Debug;
use string_cache::DefaultAtom as Atom;

/// The behavior of a encoding configuration.
pub trait EncodingConfiguration<E> {
    fn codec(&self) -> &E;
    /// If `Some(_)` this configuration will filter out any field not listed.
    fn only_fields(&self) -> &Option<Vec<Atom>>;
    /// If `Some(_)` this configuration will filter out any field listed.
    fn except_fields(&self) -> &Option<Vec<Atom>>;
    /// If `Some(_)` this configuration will configure the timestamp output.
    fn timestamp_format(&self) -> &Option<TimestampFormat>;

    fn set_only_fields(&mut self, fields: Option<Vec<Atom>>) -> Option<Vec<Atom>>;
    fn set_except_fields(&mut self, fields: Option<Vec<Atom>>) -> Option<Vec<Atom>>;
    fn set_timestamp_format(&mut self, format: Option<TimestampFormat>) -> Option<TimestampFormat>;

    fn apply_only_fields(&self, event: &mut Event) {
        if let Some(only_fields) = &self.only_fields() {
            match event {
                Event::Log(log_event) => {
                    let to_remove = log_event
                        .keys()
                        .filter(|f| !only_fields.contains(f))
                        .collect::<VecDeque<_>>();
                    for removal in to_remove {
                        log_event.remove(&removal);
                    }
                }
                Event::Metric(_) => {
                    // Metrics don't get affected by this one!
                }
            }
        }
    }
    fn apply_except_fields(&self, event: &mut Event) {
        if let Some(except_fields) = &self.except_fields() {
            match event {
                Event::Log(log_event) => {
                    for field in except_fields {
                        log_event.remove(field);
                    }
                }
                Event::Metric(_) => (), // Metrics don't get affected by this one!
            }
        }
    }
    fn apply_timestamp_format(&self, event: &mut Event) {
        if let Some(timestamp_format) = &self.timestamp_format() {
            match event {
                Event::Log(log_event) => {
                    match timestamp_format {
                        TimestampFormat::Unix => {
                            let mut unix_timestamps = Vec::new();
                            for (k, v) in log_event.all_fields() {
                                if let Value::Timestamp(ts) = v {
                                    unix_timestamps
                                        .push((k.clone(), Value::Integer(ts.timestamp())));
                                }
                            }
                            for (k, v) in unix_timestamps {
                                log_event.insert(k, v);
                            }
                        }
                        // RFC3339 is the default serialization of a timestamp.
                        TimestampFormat::RFC3339 => (),
                    }
                }
                Event::Metric(_) => (), // Metrics don't get affected by this one!
            }
        }
    }

    /// Check that the configuration is valid.
    ///
    /// If an error is returned, the entire encoding configuration should be considered inoperable.
    ///
    /// For example, this checks if `except_fields` and `only_fields` items are mutually exclusive.
    fn validate(&self) -> Result<()> {
        if let (Some(only_fields), Some(except_fields)) =
            (&self.only_fields(), &self.except_fields())
        {
            if only_fields.iter().any(|f| except_fields.contains(f)) {
                Err("`except_fields` and `only_fields` should be mutually exclusive.")?;
            }
        }
        Ok(())
    }

    /// Apply the rules to the provided event.
    ///
    /// Currently, this is idempotent.
    fn apply_rules(&self, event: &mut Event) {
        // Ordering in here should not matter.
        self.apply_except_fields(event);
        self.apply_only_fields(event);
        self.apply_timestamp_format(event);
    }
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TimestampFormat {
    Unix,
    RFC3339,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event;
    #[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
    enum TestEncoding {
        Snoot,
        Boop,
    }
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(deny_unknown_fields)]
    struct TestConfig {
        encoding: EncodingConfig<TestEncoding>,
    }

    const TOML_SIMPLE_STRING: &str = "
        encoding = \"Snoot\"
    ";
    #[test]
    fn config_string() {
        let config: TestConfig = toml::from_str(TOML_SIMPLE_STRING).unwrap();
        config.encoding.validate().unwrap();
        assert_eq!(config.encoding.codec(), &TestEncoding::Snoot);
    }

    const TOML_SIMPLE_STRUCT: &str = "
        encoding.codec = \"Snoot\"
        encoding.except_fields = [\"Doop\"]
        encoding.only_fields = [\"Boop\"]
    ";
    #[test]
    fn config_struct() {
        let config: TestConfig = toml::from_str(TOML_SIMPLE_STRUCT).unwrap();
        config.encoding.validate().unwrap();
        assert_eq!(config.encoding.codec(), &TestEncoding::Snoot);
        assert_eq!(config.encoding.except_fields(), &Some(vec!["Doop".into()]));
        assert_eq!(config.encoding.only_fields(), &Some(vec!["Boop".into()]));
    }

    const TOML_EXCLUSIVITY_VIOLATION: &str = "
        encoding.codec = \"Snoot\"
        encoding.except_fields = [\"Doop\"]
        encoding.only_fields = [\"Doop\"]
    ";
    #[test]
    fn exclusivity_violation() {
        let config: std::result::Result<TestConfig, _> = toml::from_str(TOML_EXCLUSIVITY_VIOLATION);
        assert!(config.is_err())
    }

    const TOML_EXCEPT_FIELD: &str = "
        encoding.codec = \"Snoot\"
        encoding.except_fields = [\"Doop\"]
    ";
    #[test]
    fn test_except() {
        let config: TestConfig = toml::from_str(TOML_EXCEPT_FIELD).unwrap();
        config.encoding.validate().unwrap();
        let mut event = Event::new_empty_log();
        {
            let log = event.as_mut_log();
            log.insert("Doop", 1);
            log.insert("Beep", 1);
        }
        config.encoding.apply_rules(&mut event);
        assert!(!event.as_mut_log().contains(&Atom::from("Doop")));
        assert!(event.as_mut_log().contains(&Atom::from("Beep")));
    }

    const TOML_ONLY_FIELD: &str = "
        encoding.codec = \"Snoot\"
        encoding.only_fields = [\"Doop\"]
    ";
    #[test]
    fn test_only() {
        let config: TestConfig = toml::from_str(TOML_ONLY_FIELD).unwrap();
        config.encoding.validate().unwrap();
        let mut event = Event::new_empty_log();
        {
            let log = event.as_mut_log();
            log.insert("Doop", 1);
            log.insert("Beep", 1);
        }
        config.encoding.apply_rules(&mut event);
        assert!(event.as_mut_log().contains(&Atom::from("Doop")));
        assert!(!event.as_mut_log().contains(&Atom::from("Beep")));
    }

    const TOML_TIMESTAMP_FORMAT: &str = "
        encoding.codec = \"Snoot\"
        encoding.timestamp_format = \"unix\"
    ";
    #[test]
    fn test_timestamp() {
        let config: TestConfig = toml::from_str(TOML_TIMESTAMP_FORMAT).unwrap();
        config.encoding.validate().unwrap();
        let mut event = Event::from("Demo");
        let timestamp = event
            .as_mut_log()
            .get(&event::log_schema().timestamp_key())
            .unwrap()
            .clone();
        let timestamp = timestamp.as_timestamp().unwrap();
        event
            .as_mut_log()
            .insert("another", Value::Timestamp(timestamp.clone()));

        config.encoding.apply_rules(&mut event);

        match event
            .as_mut_log()
            .get(&event::log_schema().timestamp_key())
            .unwrap()
        {
            Value::Integer(_) => {}
            e => panic!(
                "Timestamp was not transformed into a Unix timestamp. Was {:?}",
                e
            ),
        }
        match event.as_mut_log().get(&Atom::from("another")).unwrap() {
            Value::Integer(_) => {}
            e => panic!(
                "Timestamp was not transformed into a Unix timestamp. Was {:?}",
                e
            ),
        }
    }
}
