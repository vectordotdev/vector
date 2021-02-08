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

mod config;
pub use config::EncodingConfig;
mod with_default;
pub use with_default::EncodingConfigWithDefault;

use crate::{
    event::{PathComponent, PathIter, Value},
    Event, Result,
};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, fmt::Debug};

/// The behavior of a encoding configuration.
pub trait EncodingConfiguration<E> {
    // Required Accessors

    fn codec(&self) -> &E;
    fn schema(&self) -> &Option<String>;
    // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
    fn only_fields(&self) -> &Option<Vec<Vec<PathComponent>>>;
    fn except_fields(&self) -> &Option<Vec<String>>;
    fn timestamp_format(&self) -> &Option<TimestampFormat>;

    fn apply_only_fields(&self, event: &mut Event) {
        if let Some(only_fields) = &self.only_fields() {
            match event {
                Event::Log(log_event) => {
                    let to_remove = log_event
                        .keys()
                        .filter(|field| {
                            let field_path = PathIter::new(field).collect::<Vec<_>>();
                            !only_fields.iter().any(|only| {
                                // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
                                field_path.starts_with(&only[..])
                            })
                        })
                        .collect::<VecDeque<_>>();
                    for removal in to_remove {
                        log_event.remove(removal);
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
            if except_fields.iter().any(|f| {
                let path_iter = PathIter::new(f).collect::<Vec<_>>();
                only_fields.iter().any(|v| v == &path_iter)
            }) {
                return Err(
                    "`except_fields` and `only_fields` should be mutually exclusive.".into(),
                );
            }
        }
        Ok(())
    }

    /// Apply the EncodingConfig rules to the provided event.
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
    use crate::config::log_schema;
    use indoc::indoc;

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

    // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
    fn as_path_components(a: &str) -> Vec<PathComponent> {
        PathIter::new(a).collect()
    }

    const TOML_SIMPLE_STRING: &str = r#"encoding = "Snoot""#;
    #[test]
    fn config_string() {
        let config: TestConfig = toml::from_str(TOML_SIMPLE_STRING).unwrap();
        config.encoding.validate().unwrap();
        assert_eq!(config.encoding.codec(), &TestEncoding::Snoot);
    }

    const TOML_SIMPLE_STRUCT: &str = indoc! {r#"
        encoding.codec = "Snoot"
        encoding.except_fields = ["Doop"]
        encoding.only_fields = ["Boop"]
    "#};
    #[test]
    fn config_struct() {
        let config: TestConfig = toml::from_str(TOML_SIMPLE_STRUCT).unwrap();
        config.encoding.validate().unwrap();
        assert_eq!(config.encoding.codec, TestEncoding::Snoot);
        assert_eq!(config.encoding.except_fields, Some(vec!["Doop".into()]));
        assert_eq!(
            config.encoding.only_fields,
            Some(vec![as_path_components("Boop")])
        );
    }

    const TOML_EXCLUSIVITY_VIOLATION: &str = indoc! {r#"
        encoding.codec = "Snoot"
        encoding.except_fields = ["Doop"]
        encoding.only_fields = ["Doop"]
    "#};
    #[test]
    fn exclusivity_violation() {
        let config: std::result::Result<TestConfig, _> = toml::from_str(TOML_EXCLUSIVITY_VIOLATION);
        assert!(config.is_err())
    }

    const TOML_EXCEPT_FIELD: &str = indoc! {r#"
        encoding.codec = "Snoot"
        encoding.except_fields = ["a.b.c", "b", "c[0].y"]
    "#};
    #[test]
    fn test_except() {
        let config: TestConfig = toml::from_str(TOML_EXCEPT_FIELD).unwrap();
        config.encoding.validate().unwrap();
        let mut event = Event::new_empty_log();
        {
            let log = event.as_mut_log();
            log.insert("a", 1);
            log.insert("a.b", 1);
            log.insert("a.b.c", 1);
            log.insert("a.b.d", 1);
            log.insert("b[0]", 1);
            log.insert("b[1].x", 1);
            log.insert("c[0].x", 1);
            log.insert("c[0].y", 1);
        }
        config.encoding.apply_rules(&mut event);
        assert!(!event.as_mut_log().contains("a.b.c"));
        assert!(!event.as_mut_log().contains("b"));
        assert!(!event.as_mut_log().contains("b[1].x"));
        assert!(!event.as_mut_log().contains("c[0].y"));

        assert!(event.as_mut_log().contains("a.b.d"));
        assert!(event.as_mut_log().contains("c[0].x"));
    }

    const TOML_ONLY_FIELD: &str = indoc! {r#"
        encoding.codec = "Snoot"
        encoding.only_fields = ["a.b.c", "b", "c[0].y"]
    "#};
    #[test]
    fn test_only() {
        let config: TestConfig = toml::from_str(TOML_ONLY_FIELD).unwrap();
        config.encoding.validate().unwrap();
        let mut event = Event::new_empty_log();
        {
            let log = event.as_mut_log();
            log.insert("a", 1);
            log.insert("a.b", 1);
            log.insert("a.b.c", 1);
            log.insert("a.b.d", 1);
            log.insert("b[0]", 1);
            log.insert("b[1].x", 1);
            log.insert("c[0].x", 1);
            log.insert("c[0].y", 1);
        }
        config.encoding.apply_rules(&mut event);
        assert!(event.as_mut_log().contains("a.b.c"));
        assert!(event.as_mut_log().contains("b"));
        assert!(event.as_mut_log().contains("b[1].x"));
        assert!(event.as_mut_log().contains("c[0].y"));

        assert!(!event.as_mut_log().contains("a.b.d"));
        assert!(!event.as_mut_log().contains("c[0].x"));
    }

    const TOML_TIMESTAMP_FORMAT: &str = indoc! {r#"
        encoding.codec = "Snoot"
        encoding.timestamp_format = "unix"
    "#};
    #[test]
    fn test_timestamp() {
        let config: TestConfig = toml::from_str(TOML_TIMESTAMP_FORMAT).unwrap();
        config.encoding.validate().unwrap();
        let mut event = Event::from("Demo");
        let timestamp = event
            .as_mut_log()
            .get(log_schema().timestamp_key())
            .unwrap()
            .clone();
        let timestamp = timestamp.as_timestamp().unwrap();
        event
            .as_mut_log()
            .insert("another", Value::Timestamp(*timestamp));

        config.encoding.apply_rules(&mut event);

        match event
            .as_mut_log()
            .get(log_schema().timestamp_key())
            .unwrap()
        {
            Value::Integer(_) => {}
            e => panic!(
                "Timestamp was not transformed into a Unix timestamp. Was {:?}",
                e
            ),
        }
        match event.as_mut_log().get("another").unwrap() {
            Value::Integer(_) => {}
            e => panic!(
                "Timestamp was not transformed into a Unix timestamp. Was {:?}",
                e
            ),
        }
    }
}
