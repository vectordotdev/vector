use serde::{de, ser};
use serde_json::Value;
use std::fmt;

pub const GZIP_NONE: usize = 0;
pub const GZIP_FAST: usize = 1;
pub const GZIP_DEFAULT: usize = 6;
pub const GZIP_BEST: usize = 9;

#[derive(Debug, Derivative, Copy, Clone, Eq, PartialEq)]
#[derivative(Default)]
pub enum Compression {
    #[derivative(Default)]
    None,
    Gzip(Option<usize>),
}

impl Compression {
    pub const fn gzip_default() -> Compression {
        Compression::Gzip(None)
    }

    pub const fn content_encoding(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Gzip(_) => Some("gzip"),
        }
    }

    pub const fn extension(&self) -> &'static str {
        match self {
            Self::None => "log",
            Self::Gzip(_) => "log.gz",
        }
    }
}
impl fmt::Display for Compression {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Compression::None => write!(f, "none"),
            Compression::Gzip(ref level) => write!(f, "gzip({})", level.unwrap_or(GZIP_DEFAULT)),
        }
    }
}

#[cfg(feature = "rusoto_core")]
impl From<Compression> for rusoto_core::encoding::ContentEncoding {
    fn from(compression: Compression) -> Self {
        match compression {
            Compression::None => rusoto_core::encoding::ContentEncoding::Identity,
            Compression::Gzip(level) => {
                let level = level.unwrap_or(GZIP_DEFAULT);
                rusoto_core::encoding::ContentEncoding::Gzip(None, level as u32)
            }
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
                    _ => Err(de::Error::invalid_value(
                        de::Unexpected::Str(s),
                        &r#""none" or "gzip""#,
                    )),
                }
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut algorithm = None;
                let mut level = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        "algorithm" => {
                            if algorithm.is_some() {
                                return Err(de::Error::duplicate_field("algorithm"));
                            }
                            algorithm = Some(map.next_value::<&str>()?);
                        }
                        "level" => {
                            if level.is_some() {
                                return Err(de::Error::duplicate_field("level"));
                            }
                            level = Some(match map.next_value::<Value>()? {
                                Value::Number(level) => match level.as_u64() {
                                    Some(value) if value <= 9 => value as usize,
                                    Some(_) | None => {
                                        return Err(de::Error::invalid_value(
                                            de::Unexpected::Other(&level.to_string()),
                                            &"0, 1, 2, 3, 4, 5, 6, 7, 8 or 9",
                                        ))
                                    }
                                },
                                Value::String(level) => match level.as_str() {
                                    "none" => GZIP_NONE,
                                    "fast" => GZIP_FAST,
                                    "default" => GZIP_DEFAULT,
                                    "best" => GZIP_BEST,
                                    level => {
                                        return Err(de::Error::invalid_value(
                                            de::Unexpected::Str(level),
                                            &r#""none", "fast", "best" or "default""#,
                                        ))
                                    }
                                },
                                value => {
                                    return Err(de::Error::invalid_type(
                                        de::Unexpected::Other(&value.to_string()),
                                        &"integer or string",
                                    ));
                                }
                            });
                        }
                        _ => return Err(de::Error::unknown_field(key, &["algorithm", "level"])),
                    };
                }

                match algorithm.ok_or_else(|| de::Error::missing_field("algorithm"))? {
                    "none" => match level {
                        Some(_) => Err(de::Error::unknown_field("level", &[])),
                        None => Ok(Compression::None),
                    },
                    "gzip" => Ok(Compression::Gzip(level)),
                    algorithm => Err(de::Error::unknown_variant(algorithm, &["none", "gzip"])),
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

        let mut map = serializer.serialize_map(None)?;
        match self {
            Compression::None => map.serialize_entry("algorithm", "none")?,
            Compression::Gzip(level) => {
                map.serialize_entry("algorithm", "gzip")?;
                match level.unwrap_or(GZIP_DEFAULT) {
                    GZIP_NONE => map.serialize_entry("level", "none")?,
                    GZIP_FAST => map.serialize_entry("level", "fast")?,
                    GZIP_DEFAULT => map.serialize_entry("level", "default")?,
                    GZIP_BEST => map.serialize_entry("level", "best")?,
                    level => map.serialize_entry("level", &level)?,
                };
            }
        };
        map.end()
    }
}

#[cfg(test)]
mod test {
    use super::Compression;

    #[test]
    fn deserialization() {
        let fixtures_valid = [
            (r#""none""#, Compression::None),
            (r#"{"algorithm": "none"}"#, Compression::None),
            (r#"{"algorithm": "gzip"}"#, Compression::Gzip(None)),
            (
                r#"{"algorithm": "gzip", "level": "best"}"#,
                Compression::Gzip(Some(9)),
            ),
            (
                r#"{"algorithm": "gzip", "level": 8}"#,
                Compression::Gzip(Some(8)),
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
                r#"invalid value: string "b42", expected "none" or "gzip" at line 1 column 5"#,
            ),
            (
                r#"{"algorithm": "b42"}"#,
                r#"unknown variant `b42`, expected `none` or `gzip` at line 1 column 20"#,
            ),
            (
                r#"{"algorithm": "none", "level": "default"}"#,
                r#"unknown field `level`, there are no fields at line 1 column 41"#,
            ),
            (
                r#"{"algorithm": "gzip", "level": -1}"#,
                r#"invalid value: -1, expected 0, 1, 2, 3, 4, 5, 6, 7, 8 or 9 at line 1 column 34"#,
            ),
            (
                r#"{"algorithm": "gzip", "level": "good"}"#,
                r#"invalid value: string "good", expected "none", "fast", "best" or "default" at line 1 column 38"#,
            ),
            (
                r#"{"algorithm": "gzip", "level": {}}"#,
                r#"invalid type: {}, expected integer or string at line 1 column 34"#,
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
}
