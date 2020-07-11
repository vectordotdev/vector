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
    pub fn default_gzip() -> Compression {
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
                    "gzip" => Ok(Compression::Gzip(6)),
                    _ => Err(de::Error::invalid_value(de::Unexpected::Str(s), &self)),
                }
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut codec = None;
                let mut level = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        "codec" => {
                            if codec.is_some() {
                                return Err(de::Error::duplicate_field("codec"));
                            }
                            codec = Some(map.next_value()?);
                        }
                        "level" => {
                            if level.is_some() {
                                return Err(de::Error::duplicate_field("level"));
                            }
                            let value = map.next_value::<&str>()?;
                            level = Some(match value.to_lowercase().as_str() {
                                "none" => 0,
                                "fast" => 1,
                                "default" => 6,
                                "best" => 9,
                                level => match level.parse() {
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
                            });
                        }
                        _ => return Err(de::Error::unknown_field(key, &["codec", "level"])),
                    };
                }

                match codec.ok_or_else(|| de::Error::missing_field("codec"))? {
                    "none" => Ok(Compression::None),
                    "gzip" => Ok(Compression::Gzip(level.unwrap_or(6))),
                    codec => Err(de::Error::unknown_variant(codec, &["none", "gzip"])),
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
            Compression::None => map.serialize_entry("codec", "none")?,
            Compression::Gzip(level) => {
                map.serialize_entry("codec", "gzip")?;
                map.serialize_entry("level", level)?;
            }
        };
        map.end()
    }
}
