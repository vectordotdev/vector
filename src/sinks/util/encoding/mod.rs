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
    event::{LookupBuf, Value},
    Event, Result,
};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, fmt::Debug};

/// The behavior of a encoding configuration.
pub trait EncodingConfiguration<E> {
    // Required Accessors

    fn codec(&self) -> &E;
    fn schema(&self) -> &Option<String>;
    fn only_fields(&self) -> &Option<Vec<LookupBuf>>;
    fn except_fields(&self) -> &Option<Vec<LookupBuf>>;
    fn timestamp_format(&self) -> &Option<TimestampFormat>;

    fn apply_only_fields(&self, event: &mut Event) {
        if let Some(only_fields) = &self.only_fields() {
            match event {
                Event::Log(log_event) => {
                    let to_remove = log_event
                        .keys(true)
                        .filter(|field| {
                            !only_fields
                                .iter()
                                .any(|only| field.starts_with(only.clone_lookup()))
                        })
                        // We must clone here so we don't have a borrow into the logevent when we remove.
                        .map(|l| l.into_buf())
                        .collect::<VecDeque<_>>();
                    for removal in to_remove {
                        log_event.remove(&removal, true);
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
                        log_event.remove(field, false);
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
                            for (k, v) in log_event.pairs(true) {
                                if let Value::Timestamp(ts) = v {
                                    unix_timestamps
                                        .push((k.into_buf(), Value::Integer(ts.timestamp())));
                                }
                            }
                            for (k, v) in unix_timestamps {
                                // TODO: Fixed in https://github.com/timberio/vector/issues/2845
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
            if except_fields
                .iter()
                .any(|f| only_fields.iter().any(|v| v == f))
            {
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
    use crate::{config::log_schema, event::Lookup};

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

    const TOML_SIMPLE_STRING: &str = r#"
        encoding = "Snoot"
    "#;
    #[test]
    fn config_string() {
        let config: TestConfig = toml::from_str(TOML_SIMPLE_STRING).unwrap();
        config.encoding.validate().unwrap();
        assert_eq!(config.encoding.codec(), &TestEncoding::Snoot);
    }

    const TOML_SIMPLE_STRUCT: &str = r#"
        encoding.codec = "Snoot"
        encoding.except_fields = ["Doop"]
        encoding.only_fields = ["Boop"]
    "#;
    #[test]
    fn config_struct() {
        let config: TestConfig = toml::from_str(TOML_SIMPLE_STRUCT).unwrap();
        config.encoding.validate().unwrap();
        assert_eq!(config.encoding.codec, TestEncoding::Snoot);
        assert_eq!(config.encoding.except_fields, Some(vec!["Doop".into()]));
        assert_eq!(
            config.encoding.only_fields,
            Some(vec![LookupBuf::from("Boop")])
        );
    }

    const TOML_EXCLUSIVITY_VIOLATION: &str = r#"
        encoding.codec = "Snoot"
        encoding.except_fields = ["Doop"]
        encoding.only_fields = ["Doop"]
    "#;
    #[test]
    fn exclusivity_violation() {
        let config: std::result::Result<TestConfig, _> = toml::from_str(TOML_EXCLUSIVITY_VIOLATION);
        assert!(config.is_err())
    }

    const TOML_EXCEPT_FIELD: &str = r#"
        encoding.codec = "Snoot"
        encoding.except_fields = ["a.b.c", "b", "c[0].y"]
    "#;
    #[test]
    fn test_except() {
        crate::test_util::trace_init();
        let config: TestConfig = toml::from_str(TOML_EXCEPT_FIELD).unwrap();
        config.encoding.validate().unwrap();
        let mut event = Event::new_empty_log();
        {
            let log = event.as_mut_log();
            log.insert(LookupBuf::from_str("a.b.c").unwrap(), 1);
            log.insert(LookupBuf::from_str("a.b.d").unwrap(), 1);
            log.insert(LookupBuf::from_str("b[0]").unwrap(), 1);
            log.insert(LookupBuf::from_str("b[1].x").unwrap(), 1);
            log.insert(LookupBuf::from_str("c[0].x").unwrap(), 1);
            log.insert(LookupBuf::from_str("c[0].y").unwrap(), 1);
        }
        config.encoding.apply_rules(&mut event);
        assert!(!event
            .as_mut_log()
            .contains(Lookup::from_str("a.b.c").unwrap()));
        assert!(!event.as_mut_log().contains(Lookup::from_str("b").unwrap()));
        assert!(!event
            .as_mut_log()
            .contains(Lookup::from_str("b[1].x").unwrap()));
        assert!(!event
            .as_mut_log()
            .contains(Lookup::from_str("c[0].y").unwrap()));

        assert!(event
            .as_mut_log()
            .contains(Lookup::from_str("a.b.d").unwrap()));
        assert!(event
            .as_mut_log()
            .contains(Lookup::from_str("c[0].x").unwrap()));
    }

    const TOML_ONLY_FIELD: &str = r#"
        encoding.codec = "Snoot"
        encoding.only_fields = ["a.b.c", "b", "c[0].y"]
    "#;
    #[test]
    fn test_only() {
        crate::test_util::trace_init();
        let config: TestConfig = toml::from_str(TOML_ONLY_FIELD).unwrap();
        config.encoding.validate().unwrap();
        let mut event = Event::new_empty_log();
        {
            let log = event.as_mut_log();
            log.insert(LookupBuf::from_str("a.b.c").unwrap(), 1);
            log.insert(LookupBuf::from_str("a.b.d").unwrap(), 1);
            log.insert(LookupBuf::from_str("b[0]").unwrap(), 1);
            log.insert(LookupBuf::from_str("b[1].x").unwrap(), 1);
            log.insert(LookupBuf::from_str("c[0].x").unwrap(), 1);
            log.insert(LookupBuf::from_str("c[0].y").unwrap(), 1);
        }
        config.encoding.apply_rules(&mut event);
        assert!(event
            .as_mut_log()
            .contains(Lookup::from_str("a.b.c").unwrap()));
        assert!(event.as_mut_log().contains(Lookup::from_str("b").unwrap()));
        assert!(event
            .as_mut_log()
            .contains(Lookup::from_str("b[1].x").unwrap()));
        assert!(event
            .as_mut_log()
            .contains(Lookup::from_str("c[0].y").unwrap()));

        assert!(!event
            .as_mut_log()
            .contains(Lookup::from_str("a.b.d").unwrap()));
        assert!(!event
            .as_mut_log()
            .contains(Lookup::from_str("c[0].x").unwrap()));
    }

    const TOML_TIMESTAMP_FORMAT: &str = r#"
        encoding.codec = "Snoot"
        encoding.timestamp_format = "unix"
    "#;
    #[test]
    fn test_timestamp() {
        crate::test_util::trace_init();
        let config: TestConfig = toml::from_str(TOML_TIMESTAMP_FORMAT).unwrap();
        config.encoding.validate().unwrap();
        let mut event = Event::from("Demo");
        let timestamp = event
            .as_mut_log()
            .get(log_schema().timestamp_key())
            .unwrap()
            .clone();
        let timestamp = timestamp.as_timestamp();
        event
            .as_mut_log()
            .insert(LookupBuf::from("another"), Value::Timestamp(*timestamp));

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
        match event.as_mut_log().get(Lookup::from("another")).unwrap() {
            Value::Integer(_) => {}
            e => panic!(
                "Timestamp was not transformed into a Unix timestamp. Was {:?}",
                e
            ),
        }
    }
}
