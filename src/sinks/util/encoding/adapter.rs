#![deny(missing_docs)]

use super::{validate_fields, EncodingConfiguration, TimestampFormat};
use crate::{event::Event, serde::skip_serializing_if_default};
use codecs::encoding::{Framer, FramingConfig, Serializer, SerializerConfig};
use core::fmt::Debug;
use lookup::lookup_v2::OwnedPath;
use serde::{Deserialize, Deserializer, Serialize};
use std::marker::PhantomData;

/// Trait used to migrate from a sink-specific `Codec` enum to the new
/// `SerializerConfig` encoding configuration.
pub trait EncodingConfigMigrator {
    /// The sink-specific encoding type to be migrated.
    type Codec;

    /// Returns the serializer configuration that is functionally equivalent to the given legacy
    /// codec.
    fn migrate(codec: &Self::Codec) -> SerializerConfig;
}

/// This adapter serves to migrate sinks from the old sink-specific `EncodingConfig<T>` to the new
/// `SerializerConfig` encoding configuration while keeping backwards-compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EncodingConfigAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigMigrator<Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec>
        + Debug
        + Clone,
{
    /// The legacy sink-specific encoding configuration.
    LegacyEncodingConfig(LegacyEncodingConfigWrapper<LegacyEncodingConfig, Migrator>),
    /// The encoding configuration.
    Encoding(EncodingConfig),
}

impl<LegacyEncodingConfig, Migrator> From<LegacyEncodingConfig>
    for EncodingConfigAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigMigrator<Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec>
        + Debug
        + Clone,
{
    fn from(encoding: LegacyEncodingConfig) -> Self {
        Self::LegacyEncodingConfig(LegacyEncodingConfigWrapper {
            encoding,
            phantom: PhantomData,
        })
    }
}

impl<LegacyEncodingConfig, Migrator> EncodingConfigAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigMigrator<Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec>
        + Debug
        + Clone,
{
    /// Create a new encoding configuration.
    pub fn new(encoding: SerializerConfig) -> Self {
        Self::Encoding(EncodingConfig {
            encoding: EncodingWithTransformationConfig {
                encoding,
                only_fields: None,
                except_fields: None,
                timestamp_format: None,
            },
        })
    }

    /// Create a legacy sink-specific encoding configuration.
    pub fn legacy(encoding: LegacyEncodingConfig) -> Self {
        Self::LegacyEncodingConfig(LegacyEncodingConfigWrapper {
            encoding,
            phantom: PhantomData,
        })
    }
}

impl<LegacyEncodingConfig, Migrator> EncodingConfigAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone,
    Migrator: EncodingConfigMigrator<Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec>
        + Debug
        + Clone,
{
    /// Build a `Transformer` that applies the encoding rules to an event before serialization.
    pub fn transformer(&self) -> Transformer {
        match self {
            Self::Encoding(config) => Transformer {
                only_fields: config.encoding.only_fields.clone(),
                except_fields: config.encoding.except_fields.clone(),
                timestamp_format: config.encoding.timestamp_format,
            },
            Self::LegacyEncodingConfig(config) => Transformer {
                only_fields: config.encoding.only_fields().clone(),
                except_fields: config.encoding.except_fields().clone(),
                timestamp_format: *config.encoding.timestamp_format(),
            },
        }
    }

    /// Get the migrated configuration.
    pub fn config(&self) -> SerializerConfig {
        match self {
            Self::Encoding(config) => config.encoding.encoding.clone(),
            Self::LegacyEncodingConfig(config) => Migrator::migrate(config.encoding.codec()),
        }
    }

    /// Build the serializer for this configuration.
    pub fn encoding(&self) -> Serializer {
        match self {
            Self::Encoding(config) => config.encoding.encoding.build(),
            Self::LegacyEncodingConfig(config) => {
                Migrator::migrate(config.encoding.codec()).build()
            }
        }
    }
}

/// Trait used to migrate from a sink-specific `Codec` enum to the new
/// `FramingConfig`/`SerializerConfig` encoding configuration.
pub trait EncodingConfigWithFramingMigrator {
    /// The sink-specific encoding type to be migrated.
    type Codec;

    /// Returns the framing/serializer configuration that is functionally equivalent to the given
    /// legacy codec.
    fn migrate(codec: &Self::Codec) -> (Option<FramingConfig>, SerializerConfig);
}

/// This adapter serves to migrate sinks from the old sink-specific `EncodingConfig<T>` to the new
/// `FramingConfig`/`SerializerConfig` encoding configuration while keeping backwards-compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EncodingConfigWithFramingAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigWithFramingMigrator<
            Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec,
        > + Debug
        + Clone,
{
    /// The legacy sink-specific encoding configuration.
    LegacyEncodingConfig(LegacyEncodingConfigWrapper<LegacyEncodingConfig, Migrator>),
    /// The encoding configuration.
    Encoding(EncodingConfigWithFraming),
}

impl<LegacyEncodingConfig, Migrator> From<LegacyEncodingConfig>
    for EncodingConfigWithFramingAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigWithFramingMigrator<
            Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec,
        > + Debug
        + Clone,
{
    fn from(encoding: LegacyEncodingConfig) -> Self {
        Self::LegacyEncodingConfig(LegacyEncodingConfigWrapper {
            encoding,
            phantom: PhantomData,
        })
    }
}

impl<LegacyEncodingConfig, Migrator>
    EncodingConfigWithFramingAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigWithFramingMigrator<
            Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec,
        > + Debug
        + Clone,
{
    /// Create a new encoding configuration.
    pub fn new(framing: Option<FramingConfig>, encoding: SerializerConfig) -> Self {
        Self::Encoding(EncodingConfigWithFraming {
            framing,
            encoding: EncodingWithTransformationConfig {
                encoding,
                only_fields: None,
                except_fields: None,
                timestamp_format: None,
            },
        })
    }

    /// Create a legacy sink-specific encoding configuration.
    pub fn legacy(encoding: LegacyEncodingConfig) -> Self {
        Self::LegacyEncodingConfig(LegacyEncodingConfigWrapper {
            encoding,
            phantom: PhantomData,
        })
    }
}

impl<LegacyEncodingConfig, Migrator>
    EncodingConfigWithFramingAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone,
    Migrator: EncodingConfigWithFramingMigrator<
            Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec,
        > + Debug
        + Clone,
{
    /// Build a `Transformer` that applies the encoding rules to an event before serialization.
    pub fn transformer(&self) -> Transformer {
        match self {
            Self::Encoding(config) => Transformer {
                only_fields: config.encoding.only_fields.clone(),
                except_fields: config.encoding.except_fields.clone(),
                timestamp_format: config.encoding.timestamp_format,
            },
            Self::LegacyEncodingConfig(config) => Transformer {
                only_fields: config.encoding.only_fields().clone(),
                except_fields: config.encoding.except_fields().clone(),
                timestamp_format: *config.encoding.timestamp_format(),
            },
        }
    }

    /// Get the migrated configuration.
    pub fn config(&self) -> (Option<FramingConfig>, SerializerConfig) {
        match self {
            Self::Encoding(config) => {
                let config = config.clone();
                (config.framing, config.encoding.encoding)
            }
            Self::LegacyEncodingConfig(config) => Migrator::migrate(config.encoding.codec()),
        }
    }

    /// Build the framer and serializer for this configuration.
    pub fn encoding(&self) -> (Option<Framer>, Serializer) {
        let (framer, serializer) = match self {
            Self::Encoding(config) => {
                let framer = config.framing.as_ref().map(FramingConfig::build);
                let serializer = config.encoding.encoding.build();

                (framer, serializer)
            }
            Self::LegacyEncodingConfig(config) => {
                let migration = Migrator::migrate(config.encoding.codec());
                let framer = migration.0.as_ref().map(FramingConfig::build);
                let serializer = migration.1.build();

                (framer, serializer)
            }
        };

        (framer, serializer)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyEncodingConfigWrapper<EncodingConfig, Migrator> {
    encoding: EncodingConfig,
    #[serde(skip)]
    phantom: PhantomData<Migrator>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingConfig {
    encoding: EncodingWithTransformationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingConfigWithFraming {
    framing: Option<FramingConfig>,
    encoding: EncodingWithTransformationConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct EncodingWithTransformationConfigValidated(EncodingWithTransformationConfig);

impl<'de> Deserialize<'de> for EncodingWithTransformationConfigValidated {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let config: EncodingWithTransformationConfig = Deserialize::deserialize(deserializer)?;
        validate_fields(
            config.only_fields.as_deref(),
            config.except_fields.as_deref(),
        )
        .map_err(serde::de::Error::custom)?;
        Ok(Self(config))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingWithTransformationConfig {
    #[serde(flatten)]
    encoding: SerializerConfig,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    only_fields: Option<Vec<OwnedPath>>,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    except_fields: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    timestamp_format: Option<TimestampFormat>,
}

#[derive(Debug, Clone, Default)]
/// Transformations to prepare an event for serialization.
pub struct Transformer {
    only_fields: Option<Vec<OwnedPath>>,
    except_fields: Option<Vec<String>>,
    timestamp_format: Option<TimestampFormat>,
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
        let transformer = Self {
            only_fields,
            except_fields,
            timestamp_format,
        };

        validate_fields(
            transformer.only_fields.as_deref(),
            transformer.except_fields.as_deref(),
        )?;

        Ok(transformer)
    }

    /// Prepare an event for serialization by the given transformation rules.
    pub fn transform(&self, event: &mut Event) {
        self.apply_rules(event);
    }
}

impl EncodingConfiguration for Transformer {
    type Codec = ();

    fn codec(&self) -> &Self::Codec {
        &()
    }

    fn schema(&self) -> &Option<String> {
        &None
    }

    fn only_fields(&self) -> &Option<Vec<OwnedPath>> {
        &self.only_fields
    }

    fn except_fields(&self) -> &Option<Vec<String>> {
        &self.except_fields
    }

    fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.timestamp_format
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codecs::encoding::CharacterDelimitedEncoderOptions;
    use lookup::lookup_v2::parse_path;

    #[test]
    fn deserialize_encoding_with_transformation() {
        let string = r#"
            {
                "encoding": {
                    "codec": "raw_message",
                    "only_fields": ["a.b[0]"],
                    "except_fields": ["ignore_me"],
                    "timestamp_format": "unix"
                }
            }
        "#;

        let config = serde_json::from_str::<EncodingConfig>(string).unwrap();
        let encoding = config.encoding;

        assert_eq!(encoding.only_fields, Some(vec![parse_path("a.b[0]")]));
        assert_eq!(encoding.except_fields, Some(vec!["ignore_me".to_owned()]));
        assert_eq!(encoding.timestamp_format.unwrap(), TimestampFormat::Unix);
    }

    #[test]
    fn deserialize_encoding_with_framing_and_transformation() {
        let string = r#"
            {
                "framing": {
                    "method": "character_delimited",
                    "character_delimited": {
                        "delimiter": ","
                    }
                },
                "encoding": {
                    "codec": "raw_message",
                    "only_fields": ["a.b[0]"],
                    "except_fields": ["ignore_me"],
                    "timestamp_format": "unix"
                }
            }
        "#;

        let config = serde_json::from_str::<EncodingConfigWithFraming>(string).unwrap();

        assert!(matches!(
            config.framing,
            Some(FramingConfig::CharacterDelimited {
                character_delimited: CharacterDelimitedEncoderOptions { delimiter: b',' }
            })
        ));

        let encoding = config.encoding;

        assert_eq!(encoding.only_fields, Some(vec![parse_path("a.b[0]")]));
        assert_eq!(encoding.except_fields, Some(vec!["ignore_me".to_owned()]));
        assert_eq!(encoding.timestamp_format.unwrap(), TimestampFormat::Unix);
    }

    #[test]
    fn deserialize_new_config() {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
        #[serde(rename_all = "snake_case")]
        enum LegacyEncoding {
            Foo,
        }

        #[derive(Debug, Copy, Clone, Deserialize, Serialize)]
        struct Migrator;

        impl EncodingConfigMigrator for Migrator {
            type Codec = LegacyEncoding;

            fn migrate(_: &Self::Codec) -> SerializerConfig {
                panic!()
            }
        }

        let string = r#"{ "encoding": { "codec": "raw_message" } }"#;

        let config = serde_json::from_str::<
            EncodingConfigAdapter<crate::sinks::util::EncodingConfig<LegacyEncoding>, Migrator>,
        >(string)
        .unwrap();

        let encoding = match config {
            EncodingConfigAdapter::Encoding(encoding) => encoding.encoding.encoding,
            EncodingConfigAdapter::LegacyEncodingConfig(_) => panic!(),
        };

        assert!(matches!(encoding, SerializerConfig::RawMessage));
    }

    #[test]
    fn deserialize_new_config_with_framing() {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
        #[serde(rename_all = "snake_case")]
        enum LegacyEncoding {
            Foo,
        }

        #[derive(Debug, Copy, Clone, Deserialize, Serialize)]
        struct Migrator;

        impl EncodingConfigWithFramingMigrator for Migrator {
            type Codec = LegacyEncoding;

            fn migrate(_: &Self::Codec) -> (Option<FramingConfig>, SerializerConfig) {
                panic!()
            }
        }

        let string = r#"
            {
                "framing": {
                    "method": "character_delimited",
                    "character_delimited": {
                        "delimiter": ","
                    }
                },
                "encoding": {
                    "codec": "raw_message"
                }
            }
        "#;

        let config = serde_json::from_str::<
            EncodingConfigWithFramingAdapter<
                crate::sinks::util::EncodingConfig<LegacyEncoding>,
                Migrator,
            >,
        >(string)
        .unwrap();

        let config = match config {
            EncodingConfigWithFramingAdapter::Encoding(config) => config,
            EncodingConfigWithFramingAdapter::LegacyEncodingConfig(_) => panic!(),
        };

        assert!(matches!(
            config.framing,
            Some(FramingConfig::CharacterDelimited {
                character_delimited: CharacterDelimitedEncoderOptions { delimiter: b',' }
            })
        ));

        assert!(matches!(
            config.encoding.encoding,
            SerializerConfig::RawMessage
        ));
    }

    #[test]
    fn deserialize_legacy_config() {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
        #[serde(rename_all = "snake_case")]
        enum LegacyEncoding {
            Foo,
        }

        #[derive(Debug, Copy, Clone, Deserialize, Serialize)]
        struct Migrator;

        impl EncodingConfigMigrator for Migrator {
            type Codec = LegacyEncoding;

            fn migrate(_: &Self::Codec) -> SerializerConfig {
                panic!()
            }
        }

        for string in [
            r#"{ "encoding": "foo" }"#,
            r#"{ "encoding": { "codec": "foo" } }"#,
        ] {
            let config = serde_json::from_str::<
                EncodingConfigAdapter<crate::sinks::util::EncodingConfig<LegacyEncoding>, Migrator>,
            >(string)
            .unwrap();

            let encoding = match config {
                EncodingConfigAdapter::LegacyEncodingConfig(config) => config.encoding,
                EncodingConfigAdapter::Encoding(_) => panic!(),
            };

            assert!(matches!(encoding.codec(), LegacyEncoding::Foo));
        }
    }
}
