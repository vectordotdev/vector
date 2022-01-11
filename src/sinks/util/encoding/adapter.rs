#![deny(missing_docs)]

use super::{EncodingConfiguration, TimestampFormat};
use crate::{
    codecs::encoding::{Framer, FramingConfig, Serializer, SerializerConfig},
    event::{Event, PathComponent},
};
use core::fmt::Debug;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// Trait used to migrate from a sink-specific `Codec` enum to the new
/// `Box<dyn FramingConfig>`/`Box<dyn SerializerConfig>` encoding configuration.
pub trait EncodingConfigMigrator {
    /// The sink-specific encoding type to be migrated.
    type Codec;

    /// Returns the framing/serializer configuration that is functionally equivalent to the given
    /// legacy codec.
    fn migrate(codec: &Self::Codec) -> (Option<Box<dyn FramingConfig>>, Box<dyn SerializerConfig>);
}

/// This adapter serves to migrate sinks from the old sink-specific `EncodingConfig<T>` to the new
/// `Box<dyn FramingConfig>`/`Box<dyn SerializerConfig>` encoding configuration - while keeping
/// backwards-compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EncodingConfigAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigMigrator<Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec>
        + Debug
        + Clone,
{
    /// The encoding configuration.
    Encoding(EncodingConfig),
    /// The legacy sink-specific encoding configuration.
    LegacyEncodingConfig(LegacyEncodingConfigWrapper<LegacyEncodingConfig, Migrator>),
}

impl<LegacyEncodingConfig, Migrator> EncodingConfigAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigMigrator<Codec = <LegacyEncodingConfig as EncodingConfiguration>::Codec>
        + Debug
        + Clone,
{
    /// Create a new encoding configuration.
    pub fn new(
        framing: Option<Box<dyn FramingConfig>>,
        encoding: Box<dyn SerializerConfig>,
    ) -> Self {
        Self::Encoding(EncodingConfig {
            framing,
            encoding: EncodingWithTransformationConfig {
                encoding,
                filter: None,
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
            Self::Encoding(config) => {
                let only_fields = config
                    .encoding
                    .filter
                    .as_ref()
                    .and_then(|filter| match filter {
                        OnlyOrExceptFieldsConfig::OnlyFields(fields) => {
                            Some(fields.only_fields.clone())
                        }
                        _ => None,
                    });
                let except_fields =
                    config
                        .encoding
                        .filter
                        .as_ref()
                        .and_then(|filter| match filter {
                            OnlyOrExceptFieldsConfig::ExceptFields(fields) => {
                                Some(fields.except_fields.clone())
                            }
                            _ => None,
                        });
                let timestamp_format = config.encoding.timestamp_format;

                Transformer {
                    only_fields,
                    except_fields,
                    timestamp_format,
                }
            }
            Self::LegacyEncodingConfig(config) => Transformer {
                only_fields: config.encoding.only_fields().as_ref().map(|fields| {
                    fields
                        .iter()
                        .map(|field| {
                            field
                                .iter()
                                .map(|component| component.clone().into_static())
                                .collect()
                        })
                        .collect()
                }),
                except_fields: config.encoding.except_fields().clone(),
                timestamp_format: *config.encoding.timestamp_format(),
            },
        }
    }

    /// Build the framer and serializer for this configuration.
    pub fn encoding(&self) -> crate::Result<(Option<Box<dyn Framer>>, Box<dyn Serializer>)> {
        let (framer, serializer) = match self {
            Self::Encoding(config) => {
                let framer = match &config.framing {
                    Some(framing) => Some(framing.build()?),
                    None => None,
                };
                let serializer = config.encoding.encoding.build()?;

                (framer, serializer)
            }
            Self::LegacyEncodingConfig(config) => {
                let migration = Migrator::migrate(config.encoding.codec());
                let framer = match migration.0 {
                    Some(framing) => Some(framing.build()?),
                    None => None,
                };
                let serializer = migration.1.build()?;

                (framer, serializer)
            }
        };

        Ok((framer, serializer))
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
    framing: Option<Box<dyn FramingConfig>>,
    encoding: EncodingWithTransformationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingWithTransformationConfig {
    #[serde(flatten)]
    encoding: Box<dyn SerializerConfig>,
    #[serde(flatten)]
    filter: Option<OnlyOrExceptFieldsConfig>,
    timestamp_format: Option<TimestampFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OnlyOrExceptFieldsConfig {
    OnlyFields(OnlyFieldsConfig),
    ExceptFields(ExceptFieldsConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlyFieldsConfig {
    only_fields: Vec<Vec<PathComponent<'static>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptFieldsConfig {
    except_fields: Vec<String>,
}

pub struct Transformer {
    only_fields: Option<Vec<Vec<PathComponent<'static>>>>,
    except_fields: Option<Vec<String>>,
    timestamp_format: Option<TimestampFormat>,
}

impl Transformer {
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

    fn only_fields(&self) -> &Option<Vec<Vec<PathComponent>>> {
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

    #[test]
    fn deserialize_encoding_with_transformation() {
        let string = r#"
            {
                "encoding": {
                    "codec": "text",
                    "timestamp_format": "unix",
                    "except_fields": ["ignore_me"]
                }
            }
        "#;

        let config = serde_json::from_str::<EncodingConfig>(string).unwrap();
        let encoding = config.encoding;

        assert_eq!(encoding.timestamp_format.unwrap(), TimestampFormat::Unix);
        assert_eq!(
            match encoding.filter.unwrap() {
                OnlyOrExceptFieldsConfig::ExceptFields(config) => config.except_fields,
                _ => panic!(),
            },
            vec!["ignore_me".to_owned()]
        );
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

            fn migrate(
                _: &Self::Codec,
            ) -> (Option<Box<dyn FramingConfig>>, Box<dyn SerializerConfig>) {
                panic!()
            }
        }

        let string = r#"{ "encoding": { "codec": "text" } }"#;

        let config = serde_json::from_str::<
            EncodingConfigAdapter<crate::sinks::util::EncodingConfig<LegacyEncoding>, Migrator>,
        >(string)
        .unwrap();

        let encoding = match config {
            EncodingConfigAdapter::Encoding(encoding) => encoding.encoding.encoding,
            EncodingConfigAdapter::LegacyEncodingConfig(_) => panic!(),
        };

        assert_eq!(encoding.typetag_name(), "text");
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

            fn migrate(
                _: &Self::Codec,
            ) -> (Option<Box<dyn FramingConfig>>, Box<dyn SerializerConfig>) {
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
