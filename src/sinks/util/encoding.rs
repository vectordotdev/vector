//! This module holds encoding related code.
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
//!  1. Use `#[serde(deserialize_with = "EncodingConfig::from_deserializer")]` or
//!     `#[serde(deserialize_with = "EncodingConfigWithDefault::from_deserializer", default)]`
//!     to deserialize the configuration in your sink configuration.
//!  2. Call `validate()` on this config in the `build()` step of your sink.
//!  3. Call `apply_rules(&mut event)` on this config **on each event** just before it gets sent.
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

use crate::{event::Value, Event, Result};
use serde::de::{MapAccess, Visitor};
use serde::{
    de::{self, DeserializeOwned, Deserializer, IntoDeserializer},
    Deserialize, Serialize,
};
use std::collections::VecDeque;
use std::fmt::{self, Debug};
use std::marker::PhantomData;
use string_cache::DefaultAtom as Atom;

/// A structure to wrap sink encodings and enforce field privacy.
///
/// This structure **does not** assume that there is a default format. Consider
/// `EncodingConfigWithDefault<E>` instead if `E: Default`.
#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfig<E> {
    pub(crate) codec: E,
    #[serde(default)]
    pub(crate) only_fields: Option<Vec<Atom>>,
    #[serde(default)]
    pub(crate) except_fields: Option<Vec<Atom>>,
    #[serde(default)]
    pub(crate) timestamp_format: Option<TimestampFormat>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
struct Inner<E> {
    pub(crate) codec: E,
    #[serde(default)]
    pub(crate) only_fields: Option<Vec<Atom>>,
    #[serde(default)]
    pub(crate) except_fields: Option<Vec<Atom>>,
    #[serde(default)]
    pub(crate) timestamp_format: Option<TimestampFormat>,
}

impl<E> EncodingConfiguration<E> for EncodingConfig<E> {
    fn codec(&self) -> &E {
        &self.codec
    }
    fn only_fields(&self) -> &Option<Vec<Atom>> {
        &self.only_fields
    }
    fn except_fields(&self) -> &Option<Vec<Atom>> {
        &self.except_fields
    }
    fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.timestamp_format
    }
}

/// A structure to wrap sink encodings and enforce field privacy.
///
/// This structure **does** assume that there is a default format. Consider
/// `EncodingConfig<E>` instead if `E: !Default`.
#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Default)]
pub struct EncodingConfigWithDefault<E: Default + PartialEq> {
    /// The format of the encoding.
    // TODO: This is currently sink specific.
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    pub(crate) codec: E,
    /// Keep only the following fields of the message. (Items mutually exclusive with `except_fields`)
    #[serde(default)]
    pub(crate) only_fields: Option<Vec<Atom>>,
    /// Remove the following fields of the message. (Items mutually exclusive with `only_fields`)
    #[serde(default)]
    pub(crate) except_fields: Option<Vec<Atom>>,
    /// Format for outgoing timestamps.
    #[serde(default)]
    pub(crate) timestamp_format: Option<TimestampFormat>,
}

/// For encodings, answers "Is it possible to skip serializing this value, because it's the
/// default?"
pub(crate) fn skip_serializing_if_default<E: Default + PartialEq>(e: &E) -> bool {
    e == &E::default()
}

impl<E: Default + PartialEq> EncodingConfiguration<E> for EncodingConfigWithDefault<E> {
    fn codec(&self) -> &E {
        &self.codec
    }
    fn only_fields(&self) -> &Option<Vec<Atom>> {
        &self.only_fields
    }
    fn except_fields(&self) -> &Option<Vec<Atom>> {
        &self.except_fields
    }
    fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.timestamp_format
    }
}

impl<E: Default + PartialEq> Into<EncodingConfig<E>> for EncodingConfigWithDefault<E> {
    fn into(self) -> EncodingConfig<E> {
        EncodingConfig {
            codec: self.codec,
            only_fields: self.only_fields,
            except_fields: self.except_fields,
            timestamp_format: self.timestamp_format,
        }
    }
}

impl<E: Default + PartialEq> Into<EncodingConfigWithDefault<E>> for EncodingConfig<E> {
    fn into(self) -> EncodingConfigWithDefault<E> {
        EncodingConfigWithDefault {
            codec: self.codec,
            only_fields: self.only_fields,
            except_fields: self.except_fields,
            timestamp_format: self.timestamp_format,
        }
    }
}

/// The behavior of a encoding configuration.
pub trait EncodingConfiguration<E>: Into<EncodingConfig<E>> + From<E> {
    // Required Accessors

    fn codec(&self) -> &E;
    fn only_fields(&self) -> &Option<Vec<Atom>>;
    fn except_fields(&self) -> &Option<Vec<Atom>>;
    fn timestamp_format(&self) -> &Option<TimestampFormat>;

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

impl<E> From<E> for EncodingConfig<E> {
    fn from(codec: E) -> Self {
        Self {
            codec: codec,
            only_fields: Default::default(),
            except_fields: Default::default(),
            timestamp_format: Default::default(),
        }
    }
}

impl<E: Default + PartialEq> From<E> for EncodingConfigWithDefault<E> {
    fn from(codec: E) -> Self {
        Self {
            codec: codec,
            only_fields: Default::default(),
            except_fields: Default::default(),
            timestamp_format: Default::default(),
        }
    }
}

impl<'de, E> Deserialize<'de> for EncodingConfig<E>
where
    E: DeserializeOwned + Serialize + Debug + Clone + PartialEq + Eq,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // This is a Visitor that forwards string types to T's `FromStr` impl and
        // forwards map types to T's `Deserialize` impl. The `PhantomData` is to
        // keep the compiler from complaining about T being an unused generic type
        // parameter. We need T in order to know the Value type for the Visitor
        // impl.
        struct StringOrStruct<T: DeserializeOwned + Serialize + Debug + Eq + PartialEq + Clone>(
            PhantomData<fn() -> T>,
        );

        impl<'de, T> Visitor<'de> for StringOrStruct<T>
        where
            T: DeserializeOwned + Serialize + Debug + Eq + PartialEq + Clone,
        {
            type Value = Inner<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string or map")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value {
                    codec: T::deserialize(value.into_deserializer())?,
                    only_fields: Default::default(),
                    except_fields: Default::default(),
                    timestamp_format: Default::default(),
                })
            }

            fn visit_map<M>(self, map: M) -> std::result::Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                // `MapAccessDeserializer` is a wrapper that turns a `MapAccess`
                // into a `Deserializer`, allowing it to be used as the input to T's
                // `Deserialize` implementation. T then deserializes itself using
                // the entries from the map visitor.
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
            }
        }

        let inner = deserializer.deserialize_any(StringOrStruct::<E>(PhantomData))?;

        Ok(Self {
            codec: inner.codec,
            only_fields: inner.only_fields,
            except_fields: inner.except_fields,
            timestamp_format: inner.timestamp_format,
        })
    }
}

impl<E> EncodingConfigWithDefault<E>
where
    E: DeserializeOwned + Serialize + Debug + Clone + PartialEq + Eq + Default,
{
    // Derived from https://serde.rs/string-or-struct.html
    #[allow(dead_code)] // For supporting `--no-default-features`
    pub(crate) fn from_deserializer<'de, D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // This is a Visitor that forwards string types to T's `FromStr` impl and
        // forwards map types to T's `Deserialize` impl. The `PhantomData` is to
        // keep the compiler from complaining about T being an unused generic type
        // parameter. We need T in order to know the Value type for the Visitor
        // impl.
        struct StringOrStruct<T: DeserializeOwned + Serialize + Debug + Eq + PartialEq + Clone + Default>(
            PhantomData<fn() -> T>,
        );

        impl<'de, T> Visitor<'de> for StringOrStruct<T>
        where
            T: DeserializeOwned + Serialize + Debug + Eq + PartialEq + Clone + Default,
        {
            type Value = EncodingConfigWithDefault<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string or map")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value {
                    codec: T::deserialize(value.into_deserializer())?,
                    only_fields: Default::default(),
                    except_fields: Default::default(),
                    timestamp_format: Default::default(),
                })
            }

            fn visit_map<M>(self, map: M) -> std::result::Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                // `MapAccessDeserializer` is a wrapper that turns a `MapAccess`
                // into a `Deserializer`, allowing it to be used as the input to T's
                // `Deserialize` implementation. T then deserializes itself using
                // the entries from the map visitor.
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
            }
        }

        deserializer.deserialize_any(StringOrStruct(PhantomData))
    }
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
        assert_eq!(config.encoding.codec, TestEncoding::Snoot);
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
        assert_eq!(config.encoding.codec, TestEncoding::Snoot);
        assert_eq!(config.encoding.except_fields, Some(vec!["Doop".into()]));
        assert_eq!(config.encoding.only_fields, Some(vec!["Boop".into()]));
    }

    const TOML_EXCLUSIVITY_VIOLATION: &str = "
        encoding.codec = \"Snoot\"
        encoding.except_fields = [\"Doop\"]
        encoding.only_fields = [\"Doop\"]
    ";
    #[test]
    fn exclusivity_violation() {
        let config: TestConfig = toml::from_str(TOML_EXCLUSIVITY_VIOLATION).unwrap();
        assert!(config.encoding.validate().is_err());
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
