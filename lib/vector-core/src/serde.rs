use std::{fmt, marker::PhantomData};

use serde::{Deserialize, Deserializer, Serialize, de};
use vector_config::Configurable;

/// Answers "Is this value in it's default state?" which can be used to skip serializing the value.
#[inline]
pub fn is_default<E: Default + PartialEq>(e: &E) -> bool {
    e == &E::default()
}

/// Enables deserializing from a value that could be a bool or a struct.
///
/// Example:
/// healthcheck: bool
/// healthcheck.enabled: bool
/// Both are accepted.
///
/// For `Configurable` fields, use the qualified `crate::serde::bool_or_struct` path
/// (or the `vector_core` / `vector_lib` re-export) so schema generation recognizes both forms.
/// Flattened fields remain object-only.
///
/// # Errors
///
/// Returns the error from deserializing the underlying struct.
pub fn bool_or_struct<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + From<bool>,
    D: Deserializer<'de>,
{
    struct BoolOrStruct<T>(PhantomData<fn() -> T>);

    impl<'de, T> de::Visitor<'de> for BoolOrStruct<T>
    where
        T: Deserialize<'de> + From<bool>,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("bool or map")
        }

        fn visit_bool<E>(self, value: bool) -> Result<T, E>
        where
            E: de::Error,
        {
            tracing::warn!(
                "DEPRECATED: Boolean shorthand for an object-valued configuration option. Use the object form instead. See https://vector.dev/deprecations/ for migration instructions."
            );
            Ok(value.into())
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(BoolOrStruct(PhantomData))
}

/// One ASCII character, represented as a string in configuration and a byte in codecs.
#[derive(Clone, Copy, Configurable, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
#[configurable(metadata(docs::type_override = "ascii_char"))]
pub struct AsciiChar(#[configurable(validation(pattern = r"^[\x00-\x7F]$"))] char);

impl AsciiChar {
    /// Creates an ASCII character from a known character, including in constants.
    ///
    /// Use [`Self::try_from`] for untrusted input.
    ///
    /// # Panics
    ///
    /// Panics if `character` is not ASCII.
    #[must_use]
    pub const fn new(character: char) -> Self {
        assert!(character.is_ascii(), "expected an ASCII character");
        Self(character)
    }

    /// Returns the single byte representing this character.
    #[must_use]
    pub const fn as_byte(self) -> u8 {
        self.0 as u8
    }
}

impl TryFrom<char> for AsciiChar {
    type Error = String;

    fn try_from(character: char) -> Result<Self, Self::Error> {
        if character.is_ascii() {
            Ok(Self::new(character))
        } else {
            Err(format!(
                "invalid character: {character}, expected character in ASCII range"
            ))
        }
    }
}

impl<'de> Deserialize<'de> for AsciiChar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(char::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use vector_config::{Configurable, configurable_component, schema::generate_root_schema};

    use super::AsciiChar;

    /// Configuration with a default ASCII character.
    #[configurable_component]
    struct Config {
        /// The delimiter.
        #[serde(default = "default_delimiter")]
        delimiter: AsciiChar,
    }

    const fn default_delimiter() -> AsciiChar {
        AsciiChar::new(',')
    }

    #[test]
    fn ascii_character_config_contract() {
        // Exhaust the ASCII range, including NUL, digits, whitespace, and DEL.
        for byte in 0..=127 {
            let character = char::from(byte);
            let input = json!(character);
            let parsed: AsciiChar = serde_json::from_value(input.clone()).unwrap();
            assert_eq!(parsed.as_byte(), byte);
            assert_eq!(parsed, AsciiChar::try_from(character).unwrap());
            assert_eq!(serde_json::to_value(parsed).unwrap(), input);
        }

        for (name, input) in [
            ("non-ASCII", json!("é")),
            ("multibyte Unicode", json!("🦀")),
            ("empty", json!("")),
            ("multiple characters", json!("ab")),
            ("numeric digit", json!(1)),
            ("numeric byte", json!(44)),
            ("boolean", json!(true)),
            ("null", Value::Null),
            ("array", json!([","])),
            ("object", json!({})),
        ] {
            assert!(
                serde_json::from_value::<AsciiChar>(input).is_err(),
                "{name}"
            );
        }
        assert!(AsciiChar::try_from('é').is_err());
        assert_eq!(AsciiChar::default().as_byte(), 0);

        let config: Config = serde_json::from_value(json!({})).unwrap();
        assert_eq!(config.delimiter.as_byte(), b',');
        let schema = serde_json::to_value(generate_root_schema::<Config>().unwrap()).unwrap();
        let property = &schema["properties"]["delimiter"];
        assert_eq!(property["default"], ",");
        assert_eq!(property["type"], "string", "{schema}");
        assert_eq!(property["minLength"], 1);
        assert_eq!(property["maxLength"], 1);
        assert_eq!(property["pattern"], r"^[\x00-\x7F]$");
    }

    /// Options accepting a boolean shorthand while retaining other defaults.
    #[derive(Clone, Debug, Deserialize, Serialize, Configurable)]
    #[configurable(deprecated)]
    #[serde(default)]
    struct Options {
        /// Whether the option is enabled.
        enabled: bool,
        /// Timeout in seconds.
        timeout: u64,
    }

    impl Default for Options {
        fn default() -> Self {
            Self {
                enabled: true,
                timeout: 10,
            }
        }
    }

    impl From<bool> for Options {
        fn from(enabled: bool) -> Self {
            Self {
                enabled,
                ..Self::default()
            }
        }
    }

    /// Configuration using the existing boolean-or-struct deserializer.
    #[derive(Debug, Deserialize, Serialize, Configurable)]
    struct BoolOrStructConfig {
        /// Configurable options.
        #[serde(default, deserialize_with = "crate::serde::bool_or_struct")]
        options: Options,
    }

    #[test]
    fn bool_or_struct_preserves_serde_contract() {
        struct Case {
            name: &'static str,
            input: Value,
            expected: Option<Value>,
        }

        for case in [
            Case {
                name: "false keeps other defaults",
                input: json!({"options": false}),
                expected: Some(json!({"enabled": false, "timeout": 10})),
            },
            Case {
                name: "true keeps other defaults",
                input: json!({"options": true}),
                expected: Some(json!({"enabled": true, "timeout": 10})),
            },
            Case {
                name: "struct sets both fields",
                input: json!({"options": {"enabled": false, "timeout": 7}}),
                expected: Some(json!({"enabled": false, "timeout": 7})),
            },
            Case {
                name: "partial struct retains defaults",
                input: json!({"options": {"timeout": 7}}),
                expected: Some(json!({"enabled": true, "timeout": 7})),
            },
            Case {
                name: "missing field retains defaults",
                input: json!({}),
                expected: Some(json!({"enabled": true, "timeout": 10})),
            },
            Case {
                name: "integer",
                input: json!({"options": 1}),
                expected: None,
            },
            Case {
                name: "float",
                input: json!({"options": 1.5}),
                expected: None,
            },
            Case {
                name: "boolean-looking string",
                input: json!({"options": "false"}),
                expected: None,
            },
            Case {
                name: "arbitrary string",
                input: json!({"options": "invalid"}),
                expected: None,
            },
            Case {
                name: "null",
                input: json!({"options": null}),
                expected: None,
            },
            Case {
                name: "array",
                input: json!({"options": [false]}),
                expected: None,
            },
        ] {
            let result = serde_json::from_value::<BoolOrStructConfig>(case.input);
            if let Some(expected) = case.expected {
                let config = result.unwrap_or_else(|error| panic!("{}: {error}", case.name));
                let serialized = serde_json::to_value(config).unwrap();
                assert_eq!(serialized, json!({"options": expected}), "{}", case.name);
                let roundtrip: BoolOrStructConfig =
                    serde_json::from_value(serialized.clone()).unwrap();
                assert_eq!(
                    serde_json::to_value(roundtrip).unwrap(),
                    serialized,
                    "{}",
                    case.name
                );
            } else {
                assert!(result.is_err(), "{}: {result:?}", case.name);
            }
        }
    }

    #[test]
    fn schema_preserves_both_forms_defaults_and_deprecation() {
        let schema =
            serde_json::to_value(generate_root_schema::<BoolOrStructConfig>().unwrap()).unwrap();
        let options = &schema["properties"]["options"];
        assert_eq!(options["default"], json!({"enabled": true, "timeout": 10}));
        let shared = options
            .get("$ref")
            .and_then(Value::as_str)
            .map_or(options, |reference| {
                schema
                    .pointer(reference.strip_prefix('#').unwrap())
                    .unwrap()
            });
        assert_eq!(shared["_metadata"]["docs::enum_tagging"], "untagged");
        assert_eq!(shared["deprecated"], true);

        let variants = shared["oneOf"].as_array().unwrap();
        let types = variants
            .iter()
            .map(|variant| {
                let resolved =
                    variant
                        .get("$ref")
                        .and_then(Value::as_str)
                        .map_or(variant, |reference| {
                            schema
                                .pointer(reference.strip_prefix('#').unwrap())
                                .unwrap()
                        });
                resolved["type"].as_str().unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(types, ["boolean", "object"]);
    }
}
