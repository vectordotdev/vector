//! Encoding related code.
//!
//! You'll find three encoding configuration types that can be used:
//!   * [`EncodingConfig<E>`]
//!   * [`EncodingConfigWithDefault<E>`]
//!   * [`EncodingConfigFixed<E>`]
//!
//! These configurations wrap up common fields that can be used via [`EncodingConfiguration`] to
//! provide filtering of fields as events are encoded.  As well, from the name, and from the type
//! `E`, they define an actual codec to use for encoding the event once any configured field rules
//! have been applied.  The codec type parameter is generic and not constrained directly, but to use
//! it with the common encoding infrastructure, you'll likely want to look at [`StandardEncodings`]
//! and [`Encoder`] to understand how it all comes together.
//!
//! ## Configuration types
//!
//! ###  [`EncodingConfig<E>`]
//!
//! This configuration type is the most common: it requires a codec to be specified in all cases,
//! and so is useful when we want the user to choose a specific codec i.e. JSON vs text.
//!
//! ### [`EncodingConfigWithDefault<E>`]
//!
//! This configuration type is practically identical to [`EncodingConfigWithDefault<E>`], except it
//! will use the `Default` implementation of `E` to create the codec if a value isn't specified in
//! the configuration when deserialized.  Similarly, it won't write the codec during serialization
//! if it's already the default value.  This is good when there's an obvious default codec to use,
//! but you still want to provide the ability to change it.
//!
//! ### [`EncodingConfigFixed<E>`]
//!
//! This configuration type is specialized.  It is typically only required when the codec for a
//! given sink is fixed.  An example of this is the Datadog Archives sink, where all output files
//! must be encoded via JSON.  There's no reason for us to make a user specify that in a
//! configuration every time, and on the flip side, there's no way or reason for them to pass in
//! anything other than JSON, so we simply skip serializing and deserializing the codec altogether.
//!
//! This requires that `E` implement `Default`, as we always use the `Default` value when deserializing.
//!
//! ## Using a configuration
//!
//! Using one of the encoding configuration types involves utilizing their implementation of the
//! [`EncodingConfiguration`] trait which defines default methods for interpreting the configuration
//! of the encoding -- "only fields", "timestamp format", etc -- and applying it to a given [`Event`].
//!
//! This can be done simply by calling [`EncodingConfiguration::apply_rules`] on an [`Event`], which
//! applies all configured rules.  This should be done before actual encoding the event via the
//! specific codec.  If you're taking advantage of the implementations of [`Encoder<T>`]  for
//! [`EncodingConfiguration`], this is handled automatically for you.
//!
//! ## Implementation notes
//!
//! You may wonder why we have three different types! **Great question.** `serde` works with the
//! static `*SinkConfig` types when it deserializes our configuration. This means `serde` needs to
//! statically be aware if there is a default for some given `E` of the config. Since
//! We don't require `E: Default` we can't always assume that, so we need to create statically
//! distinct types! Having [`EncodingConfigWithDefault`] is a relatively straightforward way to
//! accomplish this without a bunch of magic.  [`EncodingConfigFixed`] goes a step further and
//! provides a way to force a codec, disallowing an override from being specified.
#[cfg(feature = "codecs")]
mod adapter;
mod codec;
mod config;
mod fixed;
mod with_default;

use std::{fmt::Debug, io, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    event::{Event, LogEvent, MaybeAsLogMut, PathComponent, PathIter, Value},
    Result,
};

#[cfg(feature = "codecs")]
pub use adapter::{EncodingConfigAdapter, EncodingConfigMigrator, Transformer};
pub use codec::{as_tracked_write, StandardEncodings, StandardJsonEncoding, StandardTextEncoding};
pub use config::EncodingConfig;
pub use fixed::EncodingConfigFixed;
pub use with_default::EncodingConfigWithDefault;

pub trait Encoder<T> {
    /// Encodes the input into the provided writer.
    ///
    /// # Errors
    ///
    /// If an I/O error is encountered while encoding the input, an error variant will be returned.
    fn encode_input(&self, input: T, writer: &mut dyn io::Write) -> io::Result<usize>;

    /// Encodes the input into a String.
    ///
    /// # Errors
    ///
    /// If an I/O error is encountered while encoding the input, an error variant will be returned.
    fn encode_input_to_string(&self, input: T) -> io::Result<String> {
        let mut buffer = vec![];
        self.encode_input(input, &mut buffer)?;
        Ok(String::from_utf8_lossy(&buffer).to_string())
    }
}

impl<E, T> Encoder<T> for Arc<E>
where
    E: Encoder<T>,
{
    fn encode_input(&self, input: T, writer: &mut dyn io::Write) -> io::Result<usize> {
        (**self).encode_input(input, writer)
    }
}

/// The behavior of a encoding configuration.
pub trait EncodingConfiguration {
    type Codec;
    // Required Accessors

    fn codec(&self) -> &Self::Codec;
    fn schema(&self) -> &Option<String>;
    // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
    fn only_fields(&self) -> &Option<Vec<Vec<PathComponent>>>;
    fn except_fields(&self) -> &Option<Vec<String>>;
    fn timestamp_format(&self) -> &Option<TimestampFormat>;

    fn apply_only_fields(&self, log: &mut LogEvent) {
        if let Some(only_fields) = &self.only_fields() {
            let mut to_remove = log
                .keys()
                .filter(|field| {
                    let field_path = PathIter::new(field).collect::<Vec<_>>();
                    !only_fields.iter().any(|only| {
                        // TODO(2410): Using PathComponents here is a hack for #2407, #2410 should fix this fully.
                        field_path.starts_with(&only[..])
                    })
                })
                .collect::<Vec<_>>();

            // reverse sort so that we delete array elements at the end first rather than
            // the start so that any `nulls` at the end are dropped and empty arrays are
            // pruned
            to_remove.sort_by(|a, b| b.cmp(a));

            for removal in to_remove {
                log.remove_prune(removal, true);
            }
        }
    }
    fn apply_except_fields(&self, log: &mut LogEvent) {
        if let Some(except_fields) = &self.except_fields() {
            for field in except_fields {
                log.remove(field);
            }
        }
    }
    fn apply_timestamp_format(&self, log: &mut LogEvent) {
        if let Some(timestamp_format) = &self.timestamp_format() {
            match timestamp_format {
                TimestampFormat::Unix => {
                    let mut unix_timestamps = Vec::new();
                    for (k, v) in log.all_fields() {
                        if let Value::Timestamp(ts) = v {
                            unix_timestamps.push((k.clone(), Value::Integer(ts.timestamp())));
                        }
                    }
                    for (k, v) in unix_timestamps {
                        log.insert(k, v);
                    }
                }
                // RFC3339 is the default serialization of a timestamp.
                TimestampFormat::Rfc3339 => (),
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
    fn apply_rules<T>(&self, event: &mut T)
    where
        T: MaybeAsLogMut,
    {
        // No rules are currently applied to metrics
        if let Some(log) = event.maybe_as_log_mut() {
            // Ordering in here should not matter.
            self.apply_except_fields(log);
            self.apply_only_fields(log);
            self.apply_timestamp_format(log);
        }
    }
}

// These types of traits will likely move into some kind of event container once the
// event layout is refactored, but trying it out here for now.
// Ideally this would return an iterator, but that's not the easiest thing to make generic
pub trait VisitLogMut {
    fn visit_logs_mut<F>(&mut self, func: F)
    where
        F: Fn(&mut LogEvent);
}

impl<T> VisitLogMut for Vec<T>
where
    T: VisitLogMut,
{
    fn visit_logs_mut<F>(&mut self, func: F)
    where
        F: Fn(&mut LogEvent),
    {
        for item in self {
            item.visit_logs_mut(&func);
        }
    }
}

impl VisitLogMut for Event {
    fn visit_logs_mut<F>(&mut self, func: F)
    where
        F: Fn(&mut LogEvent),
    {
        if let Event::Log(log_event) = self {
            func(log_event)
        }
    }
}
impl VisitLogMut for LogEvent {
    fn visit_logs_mut<F>(&mut self, func: F)
    where
        F: Fn(&mut LogEvent),
    {
        func(self);
    }
}

impl<E, T> Encoder<T> for E
where
    E: EncodingConfiguration,
    E::Codec: Encoder<T>,
    T: VisitLogMut,
{
    fn encode_input(&self, mut input: T, writer: &mut dyn io::Write) -> io::Result<usize> {
        input.visit_logs_mut(|log| {
            self.apply_rules(log);
        });
        self.codec().encode_input(input, writer)
    }
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TimestampFormat {
    Unix,
    Rfc3339,
}

fn deserialize_path_components<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Vec<Vec<PathComponent<'static>>>>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let fields: Option<Vec<String>> = serde::de::Deserialize::deserialize(deserializer)?;
    Ok(fields.map(|fields| {
        fields
            .iter()
            .map(|only| {
                PathIter::new(only)
                    .map(|component| component.into_static())
                    .collect()
            })
            .collect()
    }))
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use vector_common::btreemap;

    use super::*;
    use crate::config::log_schema;

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
        encoding.except_fields = ["a.b.c", "b", "c[0].y", "d\\.z", "e"]
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
            log.insert("d\\.z", 1);
            log.insert("e.a", 1);
            log.insert("e.b", 1);
        }
        config.encoding.apply_rules(&mut event);
        assert!(!event.as_mut_log().contains("a.b.c"));
        assert!(!event.as_mut_log().contains("b"));
        assert!(!event.as_mut_log().contains("b[1].x"));
        assert!(!event.as_mut_log().contains("c[0].y"));
        assert!(!event.as_mut_log().contains("d\\.z"));
        assert!(!event.as_mut_log().contains("e.a"));

        assert!(event.as_mut_log().contains("a.b.d"));
        assert!(event.as_mut_log().contains("c[0].x"));
    }

    const TOML_ONLY_FIELD: &str = indoc! {r#"
        encoding.codec = "Snoot"
        encoding.only_fields = ["a.b.c", "b", "c[0].y", "g\\.z"]
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
            log.insert("d.y", 1);
            log.insert("d.z", 1);
            log.insert("e[0]", 1);
            log.insert("e[1]", 1);
            log.insert("f\\.z", 1);
            log.insert("g\\.z", 1);
            log.insert("h", btreemap! {});
            log.insert("i", Vec::<Value>::new());
        }
        config.encoding.apply_rules(&mut event);
        assert!(event.as_mut_log().contains("a.b.c"));
        assert!(event.as_mut_log().contains("b"));
        assert!(event.as_mut_log().contains("b[1].x"));
        assert!(event.as_mut_log().contains("c[0].y"));
        assert!(event.as_mut_log().contains("g\\.z"));

        assert!(!event.as_mut_log().contains("a.b.d"));
        assert!(!event.as_mut_log().contains("c[0].x"));
        assert!(!event.as_mut_log().contains("d"));
        assert!(!event.as_mut_log().contains("e"));
        assert!(!event.as_mut_log().contains("f"));
        assert!(!event.as_mut_log().contains("h"));
        assert!(!event.as_mut_log().contains("i"));
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
