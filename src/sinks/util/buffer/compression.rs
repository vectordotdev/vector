use serde::{de, ser};
use std::fmt;

#[derive(Debug, Derivative, Copy, Clone, Eq, PartialEq)]
#[derivative(Default)]
pub enum Compression {
    #[derivative(Default)]
    None,
    Gzip(usize),
}

impl Compression {
    pub const fn default_gzip() -> Compression {
        Compression::Gzip(6)
    }

    pub fn content_encoding(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Gzip(_) => Some("gzip"),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::None => "log",
            Self::Gzip(_) => "log.gz",
        }
    }
}

#[cfg(feature = "rusoto_core")]
impl From<Compression> for rusoto_core::encoding::ContentEncoding {
    fn from(compression: Compression) -> Self {
        match compression {
            Compression::None => rusoto_core::encoding::ContentEncoding::Identity,
            Compression::Gzip(level) => {
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
                    "gzip" => Ok(Compression::default_gzip()),
                    _ => Err(de::Error::invalid_value(de::Unexpected::Str(s), &self)),
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
                            level = Some(match map.next_value::<usize>() {
                                Ok(value) => value.to_string(),
                                Err(_) => map.next_value::<&str>()?.to_string(),
                            });
                        }
                        _ => return Err(de::Error::unknown_field(key, &["algorithm", "level"])),
                    };
                }

                match algorithm.ok_or_else(|| de::Error::missing_field("algorithm"))? {
                    "none" => Ok(Compression::None),
                    "gzip" => Ok(Compression::Gzip(
                        match level.unwrap_or_else(|| "default".to_owned()).as_str() {
                            "none" => 0,
                            "fast" => 1,
                            "default" => 6,
                            "best" => 9,
                            value => match value.parse::<usize>() {
                                Ok(level) if level <= 9 => level,
                                Ok(level) => {
                                    return Err(de::Error::invalid_value(
                                        de::Unexpected::Unsigned(level as u64),
                                        &self,
                                    ))
                                }
                                Err(_) => {
                                    return Err(de::Error::invalid_value(
                                        de::Unexpected::Str(value),
                                        &self,
                                    ))
                                }
                            },
                        },
                    )),
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
                match level {
                    0 => map.serialize_entry("level", "none")?,
                    1 => map.serialize_entry("level", "fast")?,
                    6 => map.serialize_entry("level", "default")?,
                    9 => map.serialize_entry("level", "best")?,
                    level => map.serialize_entry("level", level)?,
                };
            }
        };
        map.end()
    }
}
