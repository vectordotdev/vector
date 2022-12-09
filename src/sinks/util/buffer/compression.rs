use std::{collections::BTreeSet, fmt};

use indexmap::IndexMap;
use serde::{de, ser};
use vector_config::{
    schema::{
        apply_metadata, generate_const_string_schema, generate_enum_schema, generate_one_of_schema,
        generate_struct_schema, get_or_generate_schema,
    },
    schemars::{gen::SchemaGenerator, schema::SchemaObject},
    Configurable, GenerateError, Metadata,
};
use vector_config_common::attributes::CustomAttribute;

/// Compression configuration.
#[derive(Copy, Clone, Debug, Derivative, Eq, PartialEq)]
#[derivative(Default)]
pub enum Compression {
    /// No compression.
    #[derivative(Default)]
    None,

    /// [Gzip][gzip] compression.
    ///
    /// [gzip]: https://www.gzip.org/
    Gzip(CompressionLevel),

    /// [Zlib][zlib] compression.
    ///
    /// [zlib]: https://zlib.net/
    Zlib(CompressionLevel),
}

impl Compression {
    /// Gets whether or not this compression will actually compress the input.
    ///
    /// While it may be counterintuitive for "compression" to not compress, this is simply a
    /// consequence of designing a single type that may or may not compress so that we can avoid
    /// having to box writers at a higher-level.
    ///
    /// Some callers can benefit from knowing whether or not compression is actually taking place,
    /// as different size limitations may come into play.
    pub const fn is_compressed(&self) -> bool {
        !matches!(self, Compression::None)
    }

    pub const fn gzip_default() -> Compression {
        Compression::Gzip(CompressionLevel::const_default())
    }

    pub const fn zlib_default() -> Compression {
        Compression::Zlib(CompressionLevel::const_default())
    }

    pub const fn content_encoding(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Gzip(_) => Some("gzip"),
            Self::Zlib(_) => Some("deflate"),
        }
    }

    pub const fn accept_encoding(self) -> Option<&'static str> {
        match self {
            Self::Gzip(_) => Some("gzip"),
            Self::Zlib(_) => Some("deflate"),
            _ => None,
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::None => "log",
            Self::Gzip(_) => "log.gz",
            Self::Zlib(_) => "log.zz",
        }
    }

    pub const fn level(self) -> flate2::Compression {
        match self {
            Self::None => flate2::Compression::none(),
            Self::Gzip(level) | Self::Zlib(level) => level.as_flate2(),
        }
    }
}

impl fmt::Display for Compression {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Compression::None => write!(f, "none"),
            Compression::Gzip(ref level) => write!(f, "gzip({})", level.as_flate2().level()),
            Compression::Zlib(ref level) => write!(f, "zlib({})", level.as_flate2().level()),
        }
    }
}

impl<'de> de::Deserialize<'de> for Compression {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct StringOrMap;

        impl<'de> de::Visitor<'de> for StringOrMap {
            type Value = Compression;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("string or map")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match s {
                    "none" => Ok(Compression::None),
                    "gzip" => Ok(Compression::gzip_default()),
                    "zlib" => Ok(Compression::zlib_default()),
                    _ => Err(de::Error::invalid_value(
                        de::Unexpected::Str(s),
                        &r#""none" or "gzip" or "zlib""#,
                    )),
                }
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut algorithm = None;
                let mut level = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "algorithm" => {
                            if algorithm.is_some() {
                                return Err(de::Error::duplicate_field("algorithm"));
                            }
                            algorithm = Some(map.next_value::<String>()?);
                        }
                        "level" => {
                            if level.is_some() {
                                return Err(de::Error::duplicate_field("level"));
                            }
                            level = Some(map.next_value::<CompressionLevel>()?);
                        }
                        _ => return Err(de::Error::unknown_field(&key, &["algorithm", "level"])),
                    };
                }

                match algorithm
                    .ok_or_else(|| de::Error::missing_field("algorithm"))?
                    .as_str()
                {
                    "none" => match level {
                        Some(_) => Err(de::Error::unknown_field("level", &[])),
                        None => Ok(Compression::None),
                    },
                    "gzip" => Ok(Compression::Gzip(level.unwrap_or_default())),
                    "zlib" => Ok(Compression::Zlib(level.unwrap_or_default())),
                    algorithm => Err(de::Error::unknown_variant(
                        algorithm,
                        &["none", "gzip", "zlib"],
                    )),
                }
            }
        }

        deserializer.deserialize_any(StringOrMap)
    }
}

impl ser::Serialize for Compression {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        use ser::SerializeMap;

        let default_level = CompressionLevel::const_default();

        match self {
            Compression::None => serializer.serialize_str("none"),
            Compression::Gzip(gzip_level) => {
                if *gzip_level != default_level {
                    let mut map = serializer.serialize_map(None)?;
                    map.serialize_entry("algorithm", "gzip")?;
                    map.serialize_entry("level", &gzip_level)?;
                    map.end()
                } else {
                    serializer.serialize_str("gzip")
                }
            }
            Compression::Zlib(zlib_level) => {
                if *zlib_level != default_level {
                    let mut map = serializer.serialize_map(None)?;
                    map.serialize_entry("algorithm", "zlib")?;
                    map.serialize_entry("level", &zlib_level)?;
                    map.end()
                } else {
                    serializer.serialize_str("zlib")
                }
            }
        }
    }
}

// TODO: Consider an approach for generating schema of "string or object" structure used by this type.
impl Configurable for Compression {
    fn referenceable_name() -> Option<&'static str> {
        Some(std::any::type_name::<Self>())
    }

    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        metadata.set_title("Compression configuration.");
        metadata.set_description("All compression algorithms use the default compression level unless otherwise specified.");
        metadata.add_custom_attribute(CustomAttribute::kv("docs::enum_tagging", "external"));
        metadata
    }

    fn generate_schema(gen: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        const ALGORITHM_NAME: &str = "algorithm";
        const LEVEL_NAME: &str = "level";
        const LOGICAL_NAME: &str = "logical_name";
        const ENUM_TAGGING_MODE: &str = "docs::enum_tagging";

        let generate_string_schema = |logical_name: &str,
                                      title: Option<&'static str>,
                                      description: &'static str|
         -> SchemaObject {
            let mut const_schema = generate_const_string_schema(logical_name.to_lowercase());
            let mut const_metadata = Metadata::<()>::with_description(description);
            if let Some(title) = title {
                const_metadata.set_title(title);
            }
            const_metadata.add_custom_attribute(CustomAttribute::kv(LOGICAL_NAME, logical_name));
            apply_metadata(&mut const_schema, const_metadata);
            const_schema
        };

        // First, we'll create the string-only subschemas for each algorithm, and wrap those up
        // within a one-of schema.
        let mut string_metadata = Metadata::<()>::with_description("Compression algorithm.");
        string_metadata.add_custom_attribute(CustomAttribute::kv(ENUM_TAGGING_MODE, "external"));

        let none_string_subschema = generate_string_schema("None", None, "No compression.");
        let gzip_string_subschema = generate_string_schema(
            "Gzip",
            Some("[Gzip][gzip] compression."),
            "[gzip]: https://www.gzip.org/",
        );
        let zlib_string_subschema = generate_string_schema(
            "Zlib",
            Some("[Zlib]][zlib] compression."),
            "[zlib]: https://zlib.net/",
        );

        let mut all_string_oneof_subschema = generate_one_of_schema(&[
            none_string_subschema,
            gzip_string_subschema,
            zlib_string_subschema,
        ]);
        apply_metadata(&mut all_string_oneof_subschema, string_metadata.clone());

        // Next we'll create a full schema for the given algorithms.
        //
        // TODO: We're currently using all three algorithms in the enum subschema for `algorithm`,
        // but in reality, `level` is never used when the algorithm is `none`. This is _currently_
        // fine because the field is optional, and we don't use `deny_unknown_fields`, so if users
        // specify it when the algorithm is `none`: no harm, no foul.
        //
        // However, it does lead to a suboptimal schema being generated, one that sort of implies it
        // may have value when set, even if the algorithm is `none`. We do this because, otherwise,
        // it's very hard to reconcile the resolved schemas during component documentation
        // generation, where we need to be able to generate the right enum key/value pair for the
        // `none` algorithm as part of the overall set of enum values declared for the `algorithm`
        // field in the "full" schema version.
        let mut compression_level_metadata = Metadata::default();
        compression_level_metadata.set_transparent();
        let compression_level_schema =
            get_or_generate_schema::<CompressionLevel>(gen, compression_level_metadata)?;

        let mut required = BTreeSet::new();
        required.insert(ALGORITHM_NAME.to_string());

        let mut properties = IndexMap::new();
        properties.insert(
            ALGORITHM_NAME.to_string(),
            all_string_oneof_subschema.clone(),
        );
        properties.insert(LEVEL_NAME.to_string(), compression_level_schema);

        let mut full_subschema = generate_struct_schema(properties, required, None);
        let mut full_metadata = Metadata::<()>::with_description("");
        full_metadata.add_custom_attribute(CustomAttribute::flag("docs::hidden"));
        apply_metadata(&mut full_subschema, full_metadata);

        // Finally, we zip both schemas together.
        Ok(generate_one_of_schema(&[
            all_string_oneof_subschema,
            full_subschema,
        ]))
    }
}

/// Compression level.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompressionLevel(flate2::Compression);

impl CompressionLevel {
    #[cfg(test)]
    const fn new(level: u32) -> Self {
        Self(flate2::Compression::new(level))
    }

    const fn const_default() -> Self {
        Self(flate2::Compression::new(6))
    }

    const fn none() -> Self {
        Self(flate2::Compression::none())
    }

    const fn best() -> Self {
        Self(flate2::Compression::best())
    }

    const fn fast() -> Self {
        Self(flate2::Compression::fast())
    }

    pub const fn as_flate2(self) -> flate2::Compression {
        self.0
    }
}

impl<'de> de::Deserialize<'de> for CompressionLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct NumberOrString;

        impl<'de> de::Visitor<'de> for NumberOrString {
            type Value = CompressionLevel;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("number or string")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match s {
                    "none" => Ok(CompressionLevel::none()),
                    "fast" => Ok(CompressionLevel::fast()),
                    "default" => Ok(CompressionLevel::const_default()),
                    "best" => Ok(CompressionLevel::best()),
                    level => {
                        return Err(de::Error::invalid_value(
                            de::Unexpected::Str(level),
                            &r#""none", "fast", "best" or "default""#,
                        ))
                    }
                }
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Err(de::Error::invalid_value(
                    de::Unexpected::Other(&v.to_string()),
                    &"0, 1, 2, 3, 4, 5, 6, 7, 8 or 9",
                ))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v <= 9 {
                    Ok(CompressionLevel(flate2::Compression::new(v as u32)))
                } else {
                    return Err(de::Error::invalid_value(
                        de::Unexpected::Unsigned(v),
                        &"0, 1, 2, 3, 4, 5, 6, 7, 8 or 9",
                    ));
                }
            }
        }

        deserializer.deserialize_any(NumberOrString)
    }
}

impl ser::Serialize for CompressionLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        const NONE: CompressionLevel = CompressionLevel::none();
        const FAST: CompressionLevel = CompressionLevel::fast();
        const BEST: CompressionLevel = CompressionLevel::best();

        match *self {
            NONE => serializer.serialize_str("none"),
            FAST => serializer.serialize_str("fast"),
            BEST => serializer.serialize_str("best"),
            level => serializer.serialize_u64(u64::from(level.0.level())),
        }
    }
}

// TODO: Consider an approach for generating schema of "string or number" structure used by this type.
impl Configurable for CompressionLevel {
    fn referenceable_name() -> Option<&'static str> {
        Some(std::any::type_name::<Self>())
    }

    fn metadata() -> Metadata<Self> {
        let mut metadata = Metadata::default();
        metadata.set_description("Compression level.");
        metadata
    }

    fn generate_schema(_: &mut SchemaGenerator) -> Result<SchemaObject, GenerateError> {
        let string_consts = ["none", "fast", "best", "default"]
            .iter()
            .map(|s| serde_json::Value::from(*s));

        let level_consts = (0u32..=9).map(serde_json::Value::from);

        let valid_values = string_consts.chain(level_consts).collect();
        Ok(generate_enum_schema(valid_values))
    }
}

#[cfg(test)]
mod test {
    use super::{Compression, CompressionLevel};

    #[test]
    fn deserialization() {
        let fixtures_valid = [
            (r#""none""#, Compression::None),
            (
                r#""gzip""#,
                Compression::Gzip(CompressionLevel::const_default()),
            ),
            (
                r#""zlib""#,
                Compression::Zlib(CompressionLevel::const_default()),
            ),
            (r#"{"algorithm": "none"}"#, Compression::None),
            (
                r#"{"algorithm": "gzip"}"#,
                Compression::Gzip(CompressionLevel::const_default()),
            ),
            (
                r#"{"algorithm": "gzip", "level": "best"}"#,
                Compression::Gzip(CompressionLevel::best()),
            ),
            (
                r#"{"algorithm": "gzip", "level": 8}"#,
                Compression::Gzip(CompressionLevel::new(8)),
            ),
            (
                r#"{"algorithm": "zlib"}"#,
                Compression::Zlib(CompressionLevel::const_default()),
            ),
            (
                r#"{"algorithm": "zlib", "level": "best"}"#,
                Compression::Zlib(CompressionLevel::best()),
            ),
            (
                r#"{"algorithm": "zlib", "level": 8}"#,
                Compression::Zlib(CompressionLevel::new(8)),
            ),
        ];
        for (sources, result) in fixtures_valid.iter() {
            let deserialized: Result<Compression, _> = serde_json::from_str(sources);
            assert_eq!(deserialized.expect("valid source"), *result);
        }

        let fixtures_invalid = [
            (
                r#"42"#,
                r#"invalid type: integer `42`, expected string or map at line 1 column 2"#,
            ),
            (
                r#""b42""#,
                r#"invalid value: string "b42", expected "none" or "gzip" or "zlib" at line 1 column 5"#,
            ),
            (
                r#"{"algorithm": "b42"}"#,
                r#"unknown variant `b42`, expected one of `none`, `gzip`, `zlib` at line 1 column 20"#,
            ),
            (
                r#"{"algorithm": "none", "level": "default"}"#,
                r#"unknown field `level`, there are no fields at line 1 column 41"#,
            ),
            (
                r#"{"algorithm": "gzip", "level": -1}"#,
                r#"invalid value: -1, expected 0, 1, 2, 3, 4, 5, 6, 7, 8 or 9 at line 1 column 33"#,
            ),
            (
                r#"{"algorithm": "gzip", "level": "good"}"#,
                r#"invalid value: string "good", expected "none", "fast", "best" or "default" at line 1 column 37"#,
            ),
            (
                r#"{"algorithm": "gzip", "level": {}}"#,
                r#"invalid type: map, expected number or string at line 1 column 33"#,
            ),
            (
                r#"{"algorithm": "gzip", "level": "default", "key": 42}"#,
                r#"unknown field `key`, expected `algorithm` or `level` at line 1 column 47"#,
            ),
        ];
        for (source, result) in fixtures_invalid.iter() {
            let deserialized: Result<Compression, _> = serde_json::from_str(source);
            let error = deserialized.expect_err("invalid source");
            assert_eq!(error.to_string().as_str(), *result);
        }
    }

    #[test]
    fn from_and_to_value() {
        let fixtures_valid = [
            Compression::None,
            Compression::Gzip(CompressionLevel::const_default()),
            Compression::Gzip(CompressionLevel::new(7)),
            Compression::Zlib(CompressionLevel::best()),
            Compression::Zlib(CompressionLevel::new(7)),
        ];

        for v in fixtures_valid {
            // Check serialize-deserialize round trip with defaults
            let value = serde_json::to_value(v).unwrap();
            serde_json::from_value::<Compression>(value).unwrap();
        }
    }
}
