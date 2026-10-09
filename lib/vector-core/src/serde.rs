use std::{cell::RefCell, fmt, marker::PhantomData};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_with::{DeserializeAs, SerializeAs};
use vector_config::{
    Configurable, GenerateError, Metadata,
    attributes::CustomAttribute,
    constants::DOCS_META_ENUM_TAGGING,
    schema::{
        SchemaGenerator, SchemaObject, generate_bool_schema, generate_one_of_schema,
        get_or_generate_schema,
    },
};

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

/// A schema-aware adapter for fields that accept a boolean shorthand or a struct.
///
/// Deserialization follows [`bool_or_struct`], while serialization retains the
/// struct representation. Use with `#[serde_as(as = "BoolOrStruct<T>")]` so the
/// generated schema describes both accepted input forms.
pub struct BoolOrStruct<T>(PhantomData<fn() -> T>);

impl<'de, T> DeserializeAs<'de, T> for BoolOrStruct<T>
where
    T: Deserialize<'de> + From<bool>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
    {
        bool_or_struct(deserializer)
    }
}

impl<T: Serialize> SerializeAs<T> for BoolOrStruct<T> {
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.serialize(serializer)
    }
}

impl<T: Configurable + 'static> Configurable for BoolOrStruct<T> {
    fn metadata() -> Metadata {
        let mut metadata = T::metadata();
        metadata.add_custom_attribute(CustomAttribute::kv(DOCS_META_ENUM_TAGGING, "untagged"));
        metadata
    }

    fn generate_schema(
        generator: &RefCell<SchemaGenerator>,
    ) -> Result<SchemaObject, GenerateError> {
        let structure = get_or_generate_schema(&T::as_configurable_ref(), generator, None)?;
        Ok(generate_one_of_schema(&[generate_bool_schema(), structure]))
    }
}

#[cfg(test)]
mod bool_or_struct_tests {
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use serde_with::serde_as;
    use vector_config::{Configurable, schema::generate_root_schema};

    /// Options with a boolean shorthand and another independently defaulted setting.
    #[derive(Clone, Debug, Deserialize, Serialize, Configurable)]
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

    /// Configuration using the schema-aware boolean-or-struct adapter.
    #[serde_as]
    #[derive(Debug, Deserialize, Serialize, Configurable)]
    struct Adapted {
        /// Configurable options.
        #[serde_as(as = "super::BoolOrStruct<Options>")]
        #[serde(default)]
        options: Options,
    }

    #[derive(Deserialize, Serialize)]
    struct Legacy {
        #[serde(default, deserialize_with = "super::bool_or_struct")]
        options: Options,
    }

    #[test]
    fn both_input_forms_keep_the_existing_struct_output_and_defaults() {
        struct Case {
            name: &'static str,
            input: Value,
            expected: Value,
        }

        for case in [
            Case {
                name: "boolean false keeps other defaults",
                input: json!({"options": false}),
                expected: json!({"enabled": false, "timeout": 10}),
            },
            Case {
                name: "boolean true keeps other defaults",
                input: json!({"options": true}),
                expected: json!({"enabled": true, "timeout": 10}),
            },
            Case {
                name: "struct sets both fields",
                input: json!({"options": {"enabled": false, "timeout": 7}}),
                expected: json!({"enabled": false, "timeout": 7}),
            },
            Case {
                name: "partial struct retains omitted defaults",
                input: json!({"options": {"timeout": 7}}),
                expected: json!({"enabled": true, "timeout": 7}),
            },
            Case {
                name: "missing field retains the struct default",
                input: json!({}),
                expected: json!({"enabled": true, "timeout": 10}),
            },
        ] {
            let adapted: Adapted = serde_json::from_value(case.input.clone())
                .unwrap_or_else(|error| panic!("{}: {error}", case.name));
            let legacy: Legacy = serde_json::from_value(case.input).unwrap();
            let serialized = serde_json::to_value(adapted).unwrap();
            assert_eq!(serialized["options"], case.expected, "{}", case.name);
            assert_eq!(
                serialized,
                serde_json::to_value(legacy).unwrap(),
                "{}",
                case.name
            );
            let roundtrip: Adapted = serde_json::from_value(serialized.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(roundtrip).unwrap(),
                serialized,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn adapter_rejects_the_same_invalid_scalar_and_array_inputs() {
        for (name, value) in [
            ("integer", json!(1)),
            ("float", json!(1.5)),
            ("boolean-looking string", json!("false")),
            ("arbitrary string", json!("invalid")),
            ("null", Value::Null),
            ("array", json!([false])),
        ] {
            let input = json!({"options": value});
            assert!(
                serde_json::from_value::<Adapted>(input.clone()).is_err(),
                "{name}"
            );
            assert!(serde_json::from_value::<Legacy>(input).is_err(), "{name}");
        }
    }

    #[test]
    fn schema_describes_both_forms_with_an_unchanged_struct_default() {
        let schema = serde_json::to_value(generate_root_schema::<Adapted>().unwrap()).unwrap();
        let options = &schema["properties"]["options"];
        assert_eq!(options["_metadata"]["docs::enum_tagging"], "untagged");
        assert_eq!(options["default"], json!({"enabled": true, "timeout": 10}));

        let variants = options["oneOf"].as_array().unwrap();
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

/// Handling of ASCII characters in `u8` fields via `serde`s `with` attribute.
///
/// ```rust
/// # use serde::{Deserialize, Serialize};
/// use vector_core::serde::ascii_char;
///
/// #[derive(Deserialize, Serialize)]
/// struct Foo {
///    #[serde(with = "ascii_char")]
///    character: u8,
/// }
/// ```
pub mod ascii_char {
    use std::cell::RefCell;

    use serde::{Deserialize, Deserializer, Serializer, de};
    use serde_with::{DeserializeAs, SerializeAs};
    use vector_config::{
        Configurable, GenerateError, Metadata,
        attributes::CustomAttribute,
        constants::SERDE_STRING_ONLY,
        schema::{SchemaGenerator, SchemaObject, StringValidation, generate_string_schema},
    };

    /// A schema-aware adapter for storing one ASCII character in a `u8` field.
    ///
    /// Use with `#[serde_as(as = "AsciiChar")]` so schema generation describes the
    /// string accepted by serde instead of the numeric storage type.
    pub struct AsciiChar;

    impl<'de> DeserializeAs<'de, u8> for AsciiChar {
        fn deserialize_as<D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
            deserialize(deserializer)
        }
    }

    impl SerializeAs<u8> for AsciiChar {
        fn serialize_as<S: Serializer>(source: &u8, serializer: S) -> Result<S::Ok, S::Error> {
            serialize(source, serializer)
        }
    }

    impl Configurable for AsciiChar {
        fn metadata() -> Metadata {
            let mut metadata = Metadata::with_transparent(true);
            metadata.add_custom_attribute(CustomAttribute::flag(SERDE_STRING_ONLY));
            metadata
        }

        fn generate_schema(_: &RefCell<SchemaGenerator>) -> Result<SchemaObject, GenerateError> {
            let mut schema = generate_string_schema();
            schema.string = Some(Box::new(StringValidation {
                min_length: Some(1),
                max_length: Some(1),
                pattern: Some(r"^[\x00-\x7F]$".to_owned()),
            }));
            Ok(schema)
        }
    }

    /// Deserialize an ASCII character as `u8`.
    ///
    /// # Errors
    ///
    /// If the item fails to be deserialized as a character, of the character to
    /// be deserialized is not part of the ASCII range, an error is returned.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<u8, D::Error>
    where
        D: Deserializer<'de>,
    {
        let character = char::deserialize(deserializer)?;
        if character.is_ascii() {
            Ok(character as u8)
        } else {
            Err(de::Error::custom(format!(
                "invalid character: {character}, expected character in ASCII range"
            )))
        }
    }

    /// Serialize an `u8` as ASCII character.
    ///
    /// # Errors
    ///
    /// Does not error.
    pub fn serialize<S>(character: &u8, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_char(*character as char)
    }

    #[cfg(test)]
    mod tests {
        use serde::{Deserialize, Serialize};
        use serde_json::{Value, json};
        use serde_with::serde_as;
        use vector_config::{Configurable, schema::generate_root_schema};

        #[derive(Deserialize, Serialize)]
        struct Foo {
            #[serde(with = "super")]
            character: u8,
        }

        /// Configuration using the schema-aware ASCII adapter.
        #[serde_as]
        #[derive(Debug, Deserialize, Serialize, Configurable)]
        struct Adapted {
            /// One ASCII character.
            #[serde_as(as = "super::AsciiChar")]
            #[serde(default = "default_character")]
            character: u8,
        }

        const fn default_character() -> u8 {
            b','
        }

        #[test]
        fn schema_adapter_preserves_ascii_deserialization_and_serialization() {
            struct Case {
                name: &'static str,
                input: Value,
                expected: Option<u8>,
            }

            for case in [
                Case {
                    name: "comma",
                    input: json!(","),
                    expected: Some(b','),
                },
                Case {
                    name: "digit character",
                    input: json!("1"),
                    expected: Some(b'1'),
                },
                Case {
                    name: "newline",
                    input: json!("\n"),
                    expected: Some(b'\n'),
                },
                Case {
                    name: "NUL",
                    input: json!("\0"),
                    expected: Some(0),
                },
                Case {
                    name: "empty string",
                    input: json!(""),
                    expected: None,
                },
                Case {
                    name: "multiple characters",
                    input: json!("ab"),
                    expected: None,
                },
                Case {
                    name: "non-ASCII character",
                    input: json!("ß"),
                    expected: None,
                },
                Case {
                    name: "numeric byte",
                    input: json!(44),
                    expected: None,
                },
                Case {
                    name: "numeric digit",
                    input: json!(1),
                    expected: None,
                },
                Case {
                    name: "boolean",
                    input: json!(true),
                    expected: None,
                },
                Case {
                    name: "null",
                    input: Value::Null,
                    expected: None,
                },
                Case {
                    name: "array",
                    input: json!([","]),
                    expected: None,
                },
            ] {
                let input = json!({"character": case.input});
                let adapted = serde_json::from_value::<Adapted>(input.clone());
                let original = serde_json::from_value::<Foo>(input.clone());
                if let Some(expected) = case.expected {
                    let adapted = adapted.unwrap_or_else(|error| panic!("{}: {error}", case.name));
                    assert_eq!(adapted.character, expected, "{}", case.name);
                    assert_eq!(original.unwrap().character, expected, "{}", case.name);
                    assert_eq!(
                        serde_json::to_value(adapted).unwrap(),
                        input,
                        "{}",
                        case.name
                    );
                } else {
                    assert!(adapted.is_err(), "{}", case.name);
                    assert!(original.is_err(), "{}", case.name);
                }
            }
        }

        #[test]
        fn schema_adapter_describes_a_string_and_serializes_its_default_as_a_character() {
            let schema = serde_json::to_value(generate_root_schema::<Adapted>().unwrap()).unwrap();
            let character = &schema["properties"]["character"];
            assert_eq!(character["type"], "string");
            assert_eq!(character["minLength"], 1);
            assert_eq!(character["maxLength"], 1);
            assert_eq!(character["pattern"], r"^[\x00-\x7F]$");
            assert_eq!(character["default"], ",");
            assert_eq!(character["_metadata"][super::SERDE_STRING_ONLY], true);
            assert_eq!(
                serde_json::from_value::<Adapted>(json!({}))
                    .unwrap()
                    .character,
                b','
            );
        }

        #[test]
        fn test_deserialize_ascii_valid() {
            let foo = serde_json::from_str::<Foo>(r#"{ "character": "\n" }"#).unwrap();
            assert_eq!(foo.character, b'\n');
        }

        #[test]
        fn test_deserialize_ascii_invalid_range() {
            assert!(serde_json::from_str::<Foo>(r#"{ "character": "ß" }"#).is_err());
        }

        #[test]
        fn test_deserialize_ascii_invalid_character() {
            assert!(serde_json::from_str::<Foo>(r#"{ "character": 0 }"#).is_err());
        }

        #[test]
        fn test_serialize_ascii() {
            let foo = Foo { character: b'\n' };
            assert_eq!(
                serde_json::to_string(&foo).unwrap(),
                r#"{"character":"\n"}"#
            );
        }
    }
}
