use crate::{Event, Result};
use getset::{Getters, Setters};
use nom::lib::std::collections::VecDeque;
use serde::de::{MapAccess, Visitor};
use serde::{
    de::{self, DeserializeOwned, Deserializer, IntoDeserializer},
    Deserialize, Serialize,
};
use std::fmt::{self, Debug};
use std::marker::PhantomData;
use string_cache::DefaultAtom as Atom;

/// A structure to wrap sink encodings and enforce field privacy.
///
/// Currently, we don't have a defined ordering, since all options are mutually exclusive.
#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Getters, Setters)]
pub struct EncodingConfig<E> {
    pub(crate) format: E,
    #[serde(default)]
    only_fields: Option<Vec<Atom>>,
    #[serde(default)]
    except_fields: Option<Vec<Atom>>,
}

impl<E: Default> Default for EncodingConfig<E> {
    fn default() -> Self {
        Self {
            format: Default::default(),
            only_fields: Default::default(),
            except_fields: Default::default(),
        }
    }
}

impl<E> From<E> for EncodingConfig<E> {
    fn from(format: E) -> Self {
        EncodingConfig {
            format,
            only_fields: Default::default(),
            except_fields: Default::default(),
        }
    }
}

impl<E> EncodingConfig<E>
where
    E: DeserializeOwned + Serialize + Debug + Clone + PartialEq + Eq,
{
    pub(crate) fn validate(&self) -> Result<()> {
        if let (Some(only_fields), Some(except_fields)) = (&self.only_fields, &self.except_fields) {
            if only_fields.iter().any(|f| except_fields.contains(f)) {
                Err("`except_fields` and `only_fields` should be mutually exclusive.")?;
            }
        }
        Ok(())
    }
    pub(crate) fn apply_rules(&self, event: &mut Event) {
        // Ordering in here should not matter.
        self.except_fields(event);
        self.only_fields(event);
    }
    pub(crate) fn only_fields(&self, event: &mut Event) {
        if let Some(only_fields) = &self.only_fields {
            match event {
                Event::Log(log_event) => {
                    let to_remove = log_event
                        .keys()
                        .filter(|f| !only_fields.contains(f))
                        .cloned()
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
    pub(crate) fn except_fields(&self, event: &mut Event) {
        if let Some(except_fields) = &self.except_fields {
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

    // Derived from https://serde.rs/string-or-struct.html
    pub(crate) fn from_deserializer<'de, D>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error>
        where
            E: DeserializeOwned + Serialize + Debug + Clone + PartialEq + Eq,
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
            type Value = EncodingConfig<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("string or map")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
                where
                    E: de::Error,
            {
                Ok(Self::Value {
                    format: T::deserialize(value.into_deserializer())?,
                    only_fields: Default::default(),
                    except_fields: Default::default(),
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
    #[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
    enum TestEncoding {
        Snoot,
        Boop,
    }
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(deny_unknown_fields)]
    struct TestConfig {
        #[serde(deserialize_with = "EncodingConfig::from_deserializer")]
        encoding: EncodingConfig<TestEncoding>,
    }

    const TOML_SIMPLE_STRING: &str = "
        encoding = \"Snoot\"
    ";
    #[test]
    fn config_string() {
        let config: TestConfig = toml::from_str(TOML_SIMPLE_STRING).unwrap();
        config.encoding.validate().unwrap();
        assert_eq!(config.encoding.format, TestEncoding::Snoot);
    }

    const TOML_SIMPLE_STRUCT: &str = "
        encoding.format = \"Snoot\"
        encoding.except_fields = [\"Doop\"]
        encoding.only_fields = [\"Boop\"]
    ";
    #[test]
    fn config_struct() {
        let config: TestConfig = toml::from_str(TOML_SIMPLE_STRUCT).unwrap();
        config.encoding.validate().unwrap();
        assert_eq!(config.encoding.format, TestEncoding::Snoot);
        assert_eq!(config.encoding.except_fields, Some(vec!["Doop".into()]));
        assert_eq!(config.encoding.only_fields, Some(vec!["Boop".into()]));
    }

    const TOML_EXCLUSIVITY_VIOLATION: &str = "
        encoding.format = \"Snoot\"
        encoding.except_fields = [\"Doop\"]
        encoding.only_fields = [\"Doop\"]
    ";
    #[test]
    fn exclusivity_violation() {
        let config: TestConfig = toml::from_str(TOML_EXCLUSIVITY_VIOLATION).unwrap();
        assert!(config.encoding.validate().is_err());
    }

    const TOML_EXCEPT_FIELD: &str = "
        encoding.format = \"Snoot\"
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
        encoding.format = \"Snoot\"
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
}
