use codecs::encoding::{FramingConfig, SerializerConfig};
use serde::{Deserialize, Serialize};

use crate::sinks::util::encoding::Transformer;

/// Config used to build an `Encoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
// `#[serde(deny_unknown_fields)]` doesn't work when flattening internally tagged enums, see
// https://github.com/serde-rs/serde/issues/1358.
pub struct EncodingConfig {
    #[serde(flatten)]
    encoding: SerializerConfig,
    #[serde(flatten)]
    transformer: Transformer,
}

impl EncodingConfig {
    /// Creates a new `EncodingConfig` with the provided `SerializerConfig` and `Transformer`.
    pub const fn new(encoding: SerializerConfig, transformer: Transformer) -> Self {
        Self {
            encoding,
            transformer,
        }
    }

    /// Build a `Transformer` that applies the encoding rules to an event before serialization.
    pub fn transformer(&self) -> Transformer {
        self.transformer.clone()
    }

    /// Get the encoding configuration.
    pub const fn config(&self) -> &SerializerConfig {
        &self.encoding
    }
}

/// Config used to build an `Encoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfigWithFraming {
    /// The framing config.
    framing: Option<FramingConfig>,
    /// The encoding config.
    encoding: EncodingConfig,
}

impl EncodingConfigWithFraming {
    /// Creates a new `EncodingConfigWithFraming` with the provided `FramingConfig`,
    /// `SerializerConfig` and `Transformer`.
    pub const fn new(
        framing: Option<FramingConfig>,
        encoding: SerializerConfig,
        transformer: Transformer,
    ) -> Self {
        Self {
            framing,
            encoding: EncodingConfig {
                encoding,
                transformer,
            },
        }
    }

    /// Build a `Transformer` that applies the encoding rules to an event before serialization.
    pub fn transformer(&self) -> Transformer {
        self.encoding.transformer.clone()
    }

    /// Get the encoding configuration.
    pub const fn config(&self) -> (&Option<FramingConfig>, &SerializerConfig) {
        (&self.framing, &self.encoding.encoding)
    }
}

#[cfg(test)]
mod test {
    use lookup::lookup_v2::parse_path;

    use super::*;
    use crate::sinks::util::encoding::{EncodingConfiguration, TimestampFormat};

    #[test]
    fn deserialize_encoding_config() {
        let string = r#"
            {
                "codec": "json",
                "only_fields": ["a.b[0]"],
                "except_fields": ["ignore_me"],
                "timestamp_format": "unix"
            }
        "#;

        let encoding = serde_json::from_str::<EncodingConfig>(string).unwrap();
        let serializer = encoding.config();

        assert!(matches!(serializer, SerializerConfig::Json));

        let transformer = encoding.transformer();

        assert_eq!(transformer.only_fields(), &Some(vec![parse_path("a.b[0]")]));
        assert_eq!(
            transformer.except_fields(),
            &Some(vec!["ignore_me".to_owned()])
        );
        assert_eq!(transformer.timestamp_format(), &Some(TimestampFormat::Unix));
    }

    #[test]
    fn deserialize_encoding_config_with_framing() {
        let string = r#"
            {
                "framing": {
                    "method": "newline_delimited"
                },
                "encoding": {
                    "codec": "json",
                    "only_fields": ["a.b[0]"],
                    "except_fields": ["ignore_me"],
                    "timestamp_format": "unix"
                }
            }
        "#;

        let encoding = serde_json::from_str::<EncodingConfigWithFraming>(string).unwrap();
        let (framing, serializer) = encoding.config();

        assert!(matches!(framing, Some(FramingConfig::NewlineDelimited)));
        assert!(matches!(serializer, SerializerConfig::Json));

        let transformer = encoding.transformer();

        assert_eq!(transformer.only_fields(), &Some(vec![parse_path("a.b[0]")]));
        assert_eq!(
            transformer.except_fields(),
            &Some(vec!["ignore_me".to_owned()])
        );
        assert_eq!(transformer.timestamp_format(), &Some(TimestampFormat::Unix));
    }

    #[test]
    fn deserialize_encoding_config_without_framing() {
        let string = r#"
            {
                "encoding": {
                    "codec": "json",
                    "only_fields": ["a.b[0]"],
                    "except_fields": ["ignore_me"],
                    "timestamp_format": "unix"
                }
            }
        "#;

        let encoding = serde_json::from_str::<EncodingConfigWithFraming>(string).unwrap();
        let (framing, serializer) = encoding.config();

        assert!(matches!(framing, None));
        assert!(matches!(serializer, SerializerConfig::Json));

        let transformer = encoding.transformer();

        assert_eq!(transformer.only_fields(), &Some(vec![parse_path("a.b[0]")]));
        assert_eq!(
            transformer.except_fields(),
            &Some(vec!["ignore_me".to_owned()])
        );
        assert_eq!(transformer.timestamp_format(), &Some(TimestampFormat::Unix));
    }
}
