#![deny(missing_docs)]

use core::fmt::Debug;

use lookup::{
    lookup_v2::{parse_path, OwnedPath},
    path,
};
use serde::{Deserialize, Deserializer, Serialize};
use value::Value;
use vector_core::event::{LogEvent, MaybeAsLogMut};

use crate::{event::Event, serde::skip_serializing_if_default};

/// Transformations to prepare an event for serialization.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Transformer(TransformerInner);

impl<'de> Deserialize<'de> for Transformer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let transformer: TransformerInner = Deserialize::deserialize(deserializer)?;
        Self::validate_fields(
            transformer.only_fields.as_deref(),
            transformer.except_fields.as_deref(),
        )
        .map_err(serde::de::Error::custom)?;
        Ok(Self(transformer))
    }
}

impl Transformer {
    /// Creates a new `Transformer`.
    ///
    /// Returns `Err` if `only_fields` and `except_fields` fail validation, i.e. are not mutually
    /// exclusive.
    pub fn new(
        only_fields: Option<Vec<OwnedPath>>,
        except_fields: Option<Vec<String>>,
        timestamp_format: Option<TimestampFormat>,
    ) -> Result<Self, crate::Error> {
        let inner = TransformerInner {
            only_fields,
            except_fields,
            timestamp_format,
        };

        Self::validate_fields(inner.only_fields.as_deref(), inner.except_fields.as_deref())?;

        Ok(Self(inner))
    }

    /// Get the `Transformer`'s `only_fields`.
    pub const fn only_fields(&self) -> &Option<Vec<OwnedPath>> {
        &self.0.only_fields
    }

    /// Get the `Transformer`'s `except_fields`.
    pub const fn except_fields(&self) -> &Option<Vec<String>> {
        &self.0.except_fields
    }

    /// Get the `Transformer`'s `timestamp_format`.
    pub const fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.0.timestamp_format
    }

    /// Check if `except_fields` and `only_fields` items are mutually exclusive.
    ///
    /// If an error is returned, the entire encoding configuration should be considered inoperable.
    fn validate_fields(
        only_fields: Option<&[OwnedPath]>,
        except_fields: Option<&[String]>,
    ) -> crate::Result<()> {
        if let (Some(only_fields), Some(except_fields)) = (only_fields, except_fields) {
            if except_fields.iter().any(|f| {
                let path_iter = parse_path(f);
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
        if let Some(only_fields) = &self.0.only_fields {
            let mut to_remove = match log.keys() {
                Some(keys) => keys
                    .filter(|field| {
                        let field_path = parse_path(field);
                        !only_fields
                            .iter()
                            .any(|only| field_path.segments.starts_with(&only.segments[..]))
                    })
                    .collect::<Vec<_>>(),
                None => vec![],
            };

            // reverse sort so that we delete array elements at the end first rather than
            // the start so that any `nulls` at the end are dropped and empty arrays are
            // pruned
            to_remove.sort_by(|a, b| b.cmp(a));

            for removal in to_remove {
                log.remove_prune(removal.as_str(), true);
            }
        }
    }

    fn apply_except_fields(&self, log: &mut LogEvent) {
        if let Some(except_fields) = &self.0.except_fields {
            for field in except_fields {
                log.remove(field.as_str());
            }
        }
    }

    fn apply_timestamp_format(&self, log: &mut LogEvent) {
        if let Some(timestamp_format) = &self.0.timestamp_format {
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
                            log.insert(path!(), Value::Integer(ts));
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
    pub fn set_except_fields(&mut self, except_fields: Option<Vec<String>>) -> crate::Result<()> {
        let transformer = TransformerInner {
            only_fields: self.0.only_fields.clone(),
            except_fields,
            timestamp_format: self.0.timestamp_format,
        };

        Self::validate_fields(
            transformer.only_fields.as_deref(),
            transformer.except_fields.as_deref(),
        )?;

        self.0 = transformer;

        Ok(())
    }
}

impl From<TransformerInner> for Transformer {
    fn from(inner: TransformerInner) -> Self {
        Self(inner)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
struct TransformerInner {
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    only_fields: Option<Vec<OwnedPath>>,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    except_fields: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    timestamp_format: Option<TimestampFormat>,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
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
            toml::from_str(r#"only_fields = ["a.b.c", "b", "c[0].y", "g\\.z"]"#).unwrap();
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
            .get(log_schema().timestamp_key())
            .unwrap()
            .clone();
        let timestamp = timestamp.as_timestamp().unwrap();
        event
            .as_mut_log()
            .insert("another", Value::Timestamp(*timestamp));

        transformer.transform(&mut event);

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

    #[test]
    fn exclusivity_violation() {
        let config: std::result::Result<Transformer, _> = toml::from_str(indoc! {r#"
            except_fields = ["Doop"]
            only_fields = ["Doop"]
        "#});
        assert!(config.is_err())
    }
}
