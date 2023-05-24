#![deny(missing_docs)]

use core::fmt::Debug;
use std::collections::BTreeMap;

use lookup::lookup_v2::ConfigValuePath;
use lookup::{
    event_path,
    lookup_v2::{parse_value_path, OwnedValuePath},
    PathPrefix,
};
use serde::{Deserialize, Deserializer};
use vector_config::configurable_component;
use vector_core::event::{LogEvent, MaybeAsLogMut};
use vrl::value::Value;

use crate::{event::Event, serde::skip_serializing_if_default};

/// Transformations to prepare an event for serialization.
#[configurable_component(no_deser)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transformer {
    /// List of fields that are included in the encoded event.
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    only_fields: Option<Vec<ConfigValuePath>>,

    /// List of fields that are excluded from the encoded event.
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    except_fields: Option<Vec<String>>,

    /// Format used for timestamp fields.
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    timestamp_format: Option<TimestampFormat>,
}

impl<'de> Deserialize<'de> for Transformer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct TransformerInner {
            #[serde(default)]
            only_fields: Option<Vec<OwnedValuePath>>,
            #[serde(default)]
            except_fields: Option<Vec<String>>,
            #[serde(default)]
            timestamp_format: Option<TimestampFormat>,
        }

        let inner: TransformerInner = Deserialize::deserialize(deserializer)?;
        Self::new(
            inner.only_fields,
            inner.except_fields,
            inner.timestamp_format,
        )
        .map_err(serde::de::Error::custom)
    }
}

impl Transformer {
    /// Creates a new `Transformer`.
    ///
    /// Returns `Err` if `only_fields` and `except_fields` fail validation, i.e. are not mutually
    /// exclusive.
    pub fn new(
        only_fields: Option<Vec<OwnedValuePath>>,
        except_fields: Option<Vec<String>>,
        timestamp_format: Option<TimestampFormat>,
    ) -> Result<Self, crate::Error> {
        Self::validate_fields(only_fields.as_ref(), except_fields.as_ref())?;

        let only_fields = only_fields.map(|x| x.into_iter().map(ConfigValuePath).collect());
        Ok(Self {
            only_fields,
            except_fields,
            timestamp_format,
        })
    }

    /// Get the `Transformer`'s `only_fields`.
    #[cfg(test)]
    pub const fn only_fields(&self) -> &Option<Vec<ConfigValuePath>> {
        &self.only_fields
    }

    /// Get the `Transformer`'s `except_fields`.
    pub const fn except_fields(&self) -> &Option<Vec<String>> {
        &self.except_fields
    }

    /// Get the `Transformer`'s `timestamp_format`.
    pub const fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.timestamp_format
    }

    /// Check if `except_fields` and `only_fields` items are mutually exclusive.
    ///
    /// If an error is returned, the entire encoding configuration should be considered inoperable.
    fn validate_fields(
        only_fields: Option<&Vec<OwnedValuePath>>,
        except_fields: Option<&Vec<String>>,
    ) -> crate::Result<()> {
        if let (Some(only_fields), Some(except_fields)) = (only_fields, except_fields) {
            if except_fields.iter().any(|f| {
                let path_iter = parse_value_path(f).unwrap();
                only_fields.iter().any(|v| v == &path_iter)
            }) {
                return Err(
                    "`except_fields` and `only_fields` should be mutually exclusive.".into(),
                );
            }
        }
        Ok(())
    }

    /// Prepare an event for serialization by the given transformation rules.
    pub fn transform(&self, event: &mut Event) {
        // Rules are currently applied to logs only.
        if let Some(log) = event.maybe_as_log_mut() {
            // Ordering in here should not matter.
            self.apply_except_fields(log);
            self.apply_only_fields(log);
            self.apply_timestamp_format(log);
        }
    }

    fn apply_only_fields(&self, log: &mut LogEvent) {
        if let Some(only_fields) = self.only_fields.as_ref() {
            let old_value = std::mem::replace(log.value_mut(), Value::Object(BTreeMap::new()));

            for field in only_fields {
                if let Some(value) = old_value.get(field) {
                    log.insert((PathPrefix::Event, field), value.clone());
                }
            }
        }
    }

    fn apply_except_fields(&self, log: &mut LogEvent) {
        if let Some(except_fields) = self.except_fields.as_ref() {
            for field in except_fields {
                log.remove(field.as_str());
            }
        }
    }

    fn apply_timestamp_format(&self, log: &mut LogEvent) {
        if let Some(timestamp_format) = self.timestamp_format.as_ref() {
            match timestamp_format {
                TimestampFormat::Unix => {
                    if log.value().is_object() {
                        let mut unix_timestamps = Vec::new();
                        for (k, v) in log.all_fields().expect("must be an object") {
                            if let Value::Timestamp(ts) = v {
                                unix_timestamps.push((k.clone(), Value::Integer(ts.timestamp())));
                            }
                        }
                        for (k, v) in unix_timestamps {
                            log.insert(k.as_str(), v);
                        }
                    } else {
                        // root is not an object
                        let timestamp = if let Value::Timestamp(ts) = log.value() {
                            Some(ts.timestamp())
                        } else {
                            None
                        };
                        if let Some(ts) = timestamp {
                            log.insert(event_path!(), Value::Integer(ts));
                        }
                    }
                }
                // RFC3339 is the default serialization of a timestamp.
                TimestampFormat::Rfc3339 => (),
            }
        }
    }

    /// Set the `except_fields` value.
    ///
    /// Returns `Err` if the new `except_fields` fail validation, i.e. are not mutually exclusive
    /// with `only_fields`.
    #[cfg(test)]
    pub fn set_except_fields(&mut self, except_fields: Option<Vec<String>>) -> crate::Result<()> {
        Self::validate_fields(
            self.only_fields
                .clone()
                .map(|x| x.into_iter().map(|x| x.0).collect())
                .as_ref(),
            except_fields.as_ref(),
        )?;

        self.except_fields = except_fields;

        Ok(())
    }
}

#[configurable_component]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
/// The format in which a timestamp should be represented.
pub enum TimestampFormat {
    /// Represent the timestamp as a Unix timestamp.
    Unix,

    /// Represent the timestamp as a RFC 3339 timestamp.
    Rfc3339,
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use vector_core::config::log_schema;

    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn serialize() {
        let string =
            r#"{"only_fields":["a.b[0]"],"except_fields":["ignore_me"],"timestamp_format":"unix"}"#;

        let transformer = serde_json::from_str::<Transformer>(string).unwrap();

        let serialized = serde_json::to_string(&transformer).unwrap();

        assert_eq!(string, serialized);
    }

    #[test]
    fn serialize_empty() {
        let string = "{}";

        let transformer = serde_json::from_str::<Transformer>(string).unwrap();

        let serialized = serde_json::to_string(&transformer).unwrap();

        assert_eq!(string, serialized);
    }

    #[test]
    fn deserialize_and_transform_except() {
        let transformer: Transformer =
            toml::from_str(r#"except_fields = ["a.b.c", "b", "c[0].y", "d\\.z", "e"]"#).unwrap();
        let mut log = LogEvent::default();
        {
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
        let mut event = Event::from(log);
        transformer.transform(&mut event);
        assert!(!event.as_mut_log().contains("a.b.c"));
        assert!(!event.as_mut_log().contains("b"));
        assert!(!event.as_mut_log().contains("b[1].x"));
        assert!(!event.as_mut_log().contains("c[0].y"));
        assert!(!event.as_mut_log().contains("d\\.z"));
        assert!(!event.as_mut_log().contains("e.a"));

        assert!(event.as_mut_log().contains("a.b.d"));
        assert!(event.as_mut_log().contains("c[0].x"));
    }

    #[test]
    fn deserialize_and_transform_only() {
        let transformer: Transformer =
            toml::from_str(r#"only_fields = ["a.b.c", "b", "c[0].y", "\"g.z\""]"#).unwrap();
        let mut log = LogEvent::default();
        {
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
            log.insert("\"f.z\"", 1);
            log.insert("\"g.z\"", 1);
            log.insert("h", BTreeMap::new());
            log.insert("i", Vec::<Value>::new());
        }
        let mut event = Event::from(log);
        transformer.transform(&mut event);
        assert!(event.as_mut_log().contains("a.b.c"));
        assert!(event.as_mut_log().contains("b"));
        assert!(event.as_mut_log().contains("b[1].x"));
        assert!(event.as_mut_log().contains("c[0].y"));
        assert!(event.as_mut_log().contains("\"g.z\""));

        assert!(!event.as_mut_log().contains("a.b.d"));
        assert!(!event.as_mut_log().contains("c[0].x"));
        assert!(!event.as_mut_log().contains("d"));
        assert!(!event.as_mut_log().contains("e"));
        assert!(!event.as_mut_log().contains("f"));
        assert!(!event.as_mut_log().contains("h"));
        assert!(!event.as_mut_log().contains("i"));
    }

    #[test]
    fn deserialize_and_transform_timestamp() {
        let transformer: Transformer = toml::from_str(r#"timestamp_format = "unix""#).unwrap();
        let mut event = Event::Log(LogEvent::from("Demo"));
        let timestamp = event
            .as_mut_log()
            .get((
                lookup::PathPrefix::Event,
                log_schema().timestamp_key().unwrap(),
            ))
            .unwrap()
            .clone();
        let timestamp = timestamp.as_timestamp().unwrap();
        event
            .as_mut_log()
            .insert("another", Value::Timestamp(*timestamp));

        transformer.transform(&mut event);

        match event
            .as_mut_log()
            .get((
                lookup::PathPrefix::Event,
                log_schema().timestamp_key().unwrap(),
            ))
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

    #[test]
    fn exclusivity_violation() {
        let config: std::result::Result<Transformer, _> = toml::from_str(indoc! {r#"
            except_fields = ["Doop"]
            only_fields = ["Doop"]
        "#});
        assert!(config.is_err())
    }

    #[test]
    fn deny_unknown_fields() {
        // We're only checking this explicitly because of our custom deserializer arrangement to
        // make it possible to throw the exclusivity error during deserialization, to ensure that we
        // enforce this on the top-level `Transformer` type even though it has to be applied at the
        // intermediate deserialization stage, on `TransformerInner`.
        let config: std::result::Result<Transformer, _> = toml::from_str(indoc! {r#"
            onlyfields = ["Doop"]
        "#});
        assert!(config.is_err())
    }
}
