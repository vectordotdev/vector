#![deny(missing_docs)]

use core::fmt::Debug;
use std::marker::PhantomData;

use codecs::encoding::{Framer, FramingConfig, Serializer, SerializerConfig};
use lookup::lookup_v2::OwnedPath;
use serde::{Deserialize, Deserializer, Serialize};

use super::{validate_fields, EncodingConfiguration, TimestampFormat};
use crate::{event::Event, serde::skip_serializing_if_default};

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
#[derive(Debug, Clone, Serialize)]
pub struct EncodingConfigAdapter<LegacyEncodingConfig, Migrator>(
    EncodingWithTransformationConfig<LegacyEncodingConfig, Migrator>,
);

impl<'de, LegacyEncodingConfig, Migrator> Deserialize<'de>
    for EncodingConfigAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Deserialize<'de> + Debug + Clone + 'static,
    LegacyEncodingConfig::Codec: Deserialize<'de> + Debug + Clone,
    Migrator: EncodingConfigMigrator<Codec = LegacyEncodingConfig::Codec>
        + Deserialize<'de>
        + Debug
        + Clone,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Clone, Deserialize)]
        #[serde(untagged)]
        enum EncodingConfig<LegacyEncodingConfig, Migrator>
        where
            LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
            Migrator: EncodingConfigMigrator<Codec = LegacyEncodingConfig::Codec> + Debug + Clone,
        {
            /// The legacy sink-specific encoding configuration.
            LegacyEncodingConfig(LegacyEncodingConfig),
            /// The encoding configuration.
            EncodingConfig(EncodingWithTransformationConfig<LegacyEncodingConfig, Migrator>),
        }

        let inner: EncodingConfig<LegacyEncodingConfig, Migrator> =
            Deserialize::deserialize(deserializer)?;

        let config = match inner {
            EncodingConfig::LegacyEncodingConfig(config) => {
                EncodingWithTransformationConfig::<LegacyEncodingConfig, Migrator> {
                    encoding: Migrator::migrate(config.codec()),
                    only_fields: config.only_fields().clone(),
                    except_fields: config.except_fields().clone(),
                    timestamp_format: *config.timestamp_format(),
                    _marker: PhantomData,
                }
            }
            EncodingConfig::EncodingConfig(config) => config,
        };

        Ok(Self(config))
    }
}

impl<LegacyEncodingConfig, Migrator> From<LegacyEncodingConfig>
    for EncodingConfigAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigMigrator<Codec = LegacyEncodingConfig::Codec> + Debug + Clone,
{
    fn from(encoding: LegacyEncodingConfig) -> Self {
        Self(
            EncodingWithTransformationConfig::<LegacyEncodingConfig, Migrator> {
                encoding: Migrator::migrate(encoding.codec()),
                only_fields: encoding.only_fields().clone(),
                except_fields: encoding.except_fields().clone(),
                timestamp_format: *encoding.timestamp_format(),
                _marker: PhantomData,
            },
        )
    }
}

impl<LegacyEncodingConfig, Migrator> EncodingConfigAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator: EncodingConfigMigrator<Codec = LegacyEncodingConfig::Codec> + Debug + Clone,
{
    /// Create a new encoding configuration.
    pub fn new(encoding: SerializerConfig) -> Self {
        Self(
            EncodingWithTransformationConfig::<LegacyEncodingConfig, Migrator> {
                encoding,
                only_fields: None,
                except_fields: None,
                timestamp_format: None,
                _marker: PhantomData,
            },
        )
    }

    /// Create a legacy sink-specific encoding configuration.
    pub fn legacy(encoding: LegacyEncodingConfig) -> Self {
        Self::from(encoding)
    }

    /// Build a `Transformer` that applies the encoding rules to an event before serialization.
    pub fn transformer(&self) -> Transformer {
        TransformerInner {
            only_fields: self.0.only_fields.clone(),
            except_fields: self.0.except_fields.clone(),
            timestamp_format: self.0.timestamp_format,
        }
        .into()
    }

    /// Get the migrated configuration.
    pub fn config(&self) -> &SerializerConfig {
        &self.0.encoding
    }

    /// Build the serializer for this configuration.
    pub fn encoding(&self) -> Serializer {
        self.0.encoding.build()
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
#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfigWithFramingAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator:
        EncodingConfigWithFramingMigrator<Codec = LegacyEncodingConfig::Codec> + Debug + Clone,
{
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    framing: Option<FramingConfig>,
    encoding: EncodingWithTransformationConfig<LegacyEncodingConfig, Migrator>,
}

impl<'de, LegacyEncodingConfig, Migrator> Deserialize<'de>
    for EncodingConfigWithFramingAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Deserialize<'de> + Debug + Clone + 'static,
    LegacyEncodingConfig::Codec: Deserialize<'de> + Debug + Clone,
    Migrator: EncodingConfigWithFramingMigrator<Codec = LegacyEncodingConfig::Codec>
        + Deserialize<'de>
        + Debug
        + Clone,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Clone, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct EncodingWithFramingConfig<LegacyEncodingConfig, Migrator>
        where
            LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
            Migrator: EncodingConfigWithFramingMigrator<Codec = LegacyEncodingConfig::Codec>
                + Debug
                + Clone,
        {
            #[serde(default)]
            framing: Option<FramingConfig>,
            encoding: EncodingConfig<LegacyEncodingConfig, Migrator>,
        }

        #[derive(Debug, Clone, Deserialize)]
        #[serde(untagged)]
        enum EncodingConfig<LegacyEncodingConfig, Migrator>
        where
            LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
            Migrator: EncodingConfigWithFramingMigrator<Codec = LegacyEncodingConfig::Codec>
                + Debug
                + Clone,
        {
            /// The legacy sink-specific encoding configuration.
            LegacyEncodingConfig(LegacyEncodingConfig),
            /// The encoding configuration.
            Encoding(EncodingWithTransformationConfig<LegacyEncodingConfig, Migrator>),
        }

        let inner: EncodingWithFramingConfig<LegacyEncodingConfig, Migrator> =
            Deserialize::deserialize(deserializer)?;

        let (framing, encoding) = match inner.encoding {
            EncodingConfig::LegacyEncodingConfig(config) => {
                let (framing, encoding) = Migrator::migrate(config.codec());
                (
                    framing,
                    EncodingWithTransformationConfig::<LegacyEncodingConfig, Migrator> {
                        encoding,
                        only_fields: config.only_fields().clone(),
                        except_fields: config.except_fields().clone(),
                        timestamp_format: *config.timestamp_format(),
                        _marker: PhantomData,
                    },
                )
            }
            EncodingConfig::Encoding(config) => (None, config),
        };

        let framing = inner.framing.or(framing);

        Ok(Self { framing, encoding })
    }
}

impl<LegacyEncodingConfig, Migrator> From<LegacyEncodingConfig>
    for EncodingConfigWithFramingAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator:
        EncodingConfigWithFramingMigrator<Codec = LegacyEncodingConfig::Codec> + Debug + Clone,
{
    fn from(config: LegacyEncodingConfig) -> Self {
        let (framing, encoding) = Migrator::migrate(config.codec());
        Self {
            framing,
            encoding: EncodingWithTransformationConfig::<LegacyEncodingConfig, Migrator> {
                encoding,
                only_fields: config.only_fields().clone(),
                except_fields: config.except_fields().clone(),
                timestamp_format: *config.timestamp_format(),
                _marker: PhantomData,
            },
        }
    }
}

impl<LegacyEncodingConfig, Migrator>
    EncodingConfigWithFramingAdapter<LegacyEncodingConfig, Migrator>
where
    LegacyEncodingConfig: EncodingConfiguration + Debug + Clone + 'static,
    Migrator:
        EncodingConfigWithFramingMigrator<Codec = LegacyEncodingConfig::Codec> + Debug + Clone,
{
    /// Create a new encoding configuration.
    pub fn new(framing: Option<FramingConfig>, encoding: SerializerConfig) -> Self {
        Self {
            framing,
            encoding: EncodingWithTransformationConfig {
                encoding,
                only_fields: None,
                except_fields: None,
                timestamp_format: None,
                _marker: PhantomData,
            },
        }
    }

    /// Create a legacy sink-specific encoding configuration.
    pub fn legacy(encoding: LegacyEncodingConfig) -> Self {
        Self::from(encoding)
    }

    /// Build a `Transformer` that applies the encoding rules to an event before serialization.
    pub fn transformer(&self) -> Transformer {
        TransformerInner {
            only_fields: self.encoding.only_fields.clone(),
            except_fields: self.encoding.except_fields.clone(),
            timestamp_format: self.encoding.timestamp_format,
        }
        .into()
    }

    /// Get the migrated configuration.
    pub fn config(&self) -> (&Option<FramingConfig>, &SerializerConfig) {
        (&self.framing, &self.encoding.encoding)
    }

    /// Build the framer and serializer for this configuration.
    pub fn encoding(&self) -> (Option<Framer>, Serializer) {
        (
            self.framing.as_ref().map(FramingConfig::build),
            self.encoding.encoding.build(),
        )
    }
}

#[derive(Debug, Clone, Serialize)]
struct EncodingWithTransformationConfig<LegacyEncodingConfig, Migrator> {
    #[serde(flatten)]
    encoding: SerializerConfig,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    only_fields: Option<Vec<OwnedPath>>,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    except_fields: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    timestamp_format: Option<TimestampFormat>,
    #[serde(skip)]
    _marker: PhantomData<(LegacyEncodingConfig, Migrator)>,
}

impl<'de, LegacyEncodingConfig, Migrator> Deserialize<'de>
    for EncodingWithTransformationConfig<LegacyEncodingConfig, Migrator>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Clone, Deserialize)]
        // `#[serde(deny_unknown_fields)]` doesn't work when flattening internally tagged enums, see
        // https://github.com/serde-rs/serde/issues/1358.
        pub struct EncodingConfig {
            #[serde(flatten)]
            encoding: SerializerConfig,
            only_fields: Option<Vec<OwnedPath>>,
            except_fields: Option<Vec<String>>,
            timestamp_format: Option<TimestampFormat>,
        }

        let inner: EncodingConfig = Deserialize::deserialize(deserializer)?;
        validate_fields(inner.only_fields.as_deref(), inner.except_fields.as_deref())
            .map_err(serde::de::Error::custom)?;

        Ok(Self {
            encoding: inner.encoding,
            only_fields: inner.only_fields,
            except_fields: inner.except_fields,
            timestamp_format: inner.timestamp_format,
            _marker: PhantomData,
        })
    }
}

/// Transformations to prepare an event for serialization.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Transformer(TransformerInner);

impl<'de> Deserialize<'de> for Transformer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let transformer: TransformerInner = Deserialize::deserialize(deserializer)?;
        validate_fields(
            transformer.only_fields.as_deref(),
            transformer.except_fields.as_deref(),
        )
        .map_err(serde::de::Error::custom)?;
        Ok(Self(transformer))
    }
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
        let inner = TransformerInner {
            only_fields,
            except_fields,
            timestamp_format,
        };

        validate_fields(inner.only_fields.as_deref(), inner.except_fields.as_deref())?;

        Ok(Self(inner))
    }

    /// Prepare an event for serialization by the given transformation rules.
    pub fn transform(&self, event: &mut Event) {
        self.apply_rules(event);
    }

    /// Set the `except_fields` value.
    ///
    /// Returns `Err` if the new `except_fields` fail validation, i.e. are not mutually exclusive
    /// with `only_fields`.
    pub fn set_except_fields(&mut self, except_fields: Option<Vec<String>>) -> crate::Result<()> {
        let transformer = TransformerInner {
            only_fields: self.0.only_fields.clone(),
            except_fields,
            timestamp_format: self.0.timestamp_format,
        };

        validate_fields(
            transformer.only_fields.as_deref(),
            transformer.except_fields.as_deref(),
        )?;

        self.0 = transformer;

        Ok(())
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
        &self.0.only_fields
    }

    fn except_fields(&self) -> &Option<Vec<String>> {
        &self.0.except_fields
    }

    fn timestamp_format(&self) -> &Option<TimestampFormat> {
        &self.0.timestamp_format
    }
}

impl From<TransformerInner> for Transformer {
    fn from(inner: TransformerInner) -> Self {
        Self(inner)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
struct TransformerInner {
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    only_fields: Option<Vec<OwnedPath>>,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    except_fields: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "skip_serializing_if_default")]
    timestamp_format: Option<TimestampFormat>,
}

#[cfg(test)]
mod tests {
    use codecs::encoding::CharacterDelimitedEncoderOptions;
    use lookup::lookup_v2::parse_path;

    use super::*;
    use crate::sinks::util::encoding::EncodingConfig;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
    #[serde(rename_all = "snake_case")]
    enum FooLegacyEncoding {
        Foo,
    }

    #[derive(Debug, Copy, Clone, Deserialize, Serialize)]
    struct FooMigrator;

    impl EncodingConfigMigrator for FooMigrator {
        type Codec = FooLegacyEncoding;

        fn migrate(_: &Self::Codec) -> SerializerConfig {
            SerializerConfig::Json
        }
    }

    #[derive(Debug, Copy, Clone, Deserialize, Serialize)]
    struct FooWithFramingMigrator;

    impl EncodingConfigWithFramingMigrator for FooWithFramingMigrator {
        type Codec = FooLegacyEncoding;

        fn migrate(_: &Self::Codec) -> (Option<FramingConfig>, SerializerConfig) {
            (
                Some(FramingConfig::NewlineDelimited),
                SerializerConfig::Json,
            )
        }
    }

    #[test]
    fn deserialize_encoding_with_transformation() {
        let string = r#"
            {
                "codec": "raw_message",
                "only_fields": ["a.b[0]"],
                "except_fields": ["ignore_me"],
                "timestamp_format": "unix"
            }
        "#;

        let adapter = serde_json::from_str::<
            EncodingConfigAdapter<EncodingConfig<FooLegacyEncoding>, FooMigrator>,
        >(string)
        .unwrap();
        let serializer = adapter.config();

        assert!(matches!(serializer, SerializerConfig::RawMessage));

        let transformer = adapter.transformer();

        assert_eq!(transformer.only_fields(), &Some(vec![parse_path("a.b[0]")]));
        assert_eq!(
            transformer.except_fields(),
            &Some(vec!["ignore_me".to_owned()])
        );
        assert_eq!(transformer.timestamp_format(), &Some(TimestampFormat::Unix));
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

        let adapter = serde_json::from_str::<
            EncodingConfigWithFramingAdapter<
                EncodingConfig<FooLegacyEncoding>,
                FooWithFramingMigrator,
            >,
        >(string)
        .unwrap();
        let (framing, serializer) = adapter.config();

        assert!(matches!(
            framing,
            Some(FramingConfig::CharacterDelimited {
                character_delimited: CharacterDelimitedEncoderOptions { delimiter: b',' }
            })
        ));

        assert!(matches!(serializer, SerializerConfig::RawMessage));

        let transformer = adapter.transformer();

        assert_eq!(transformer.only_fields(), &Some(vec![parse_path("a.b[0]")]));
        assert_eq!(
            transformer.except_fields(),
            &Some(vec!["ignore_me".to_owned()])
        );
        assert_eq!(transformer.timestamp_format(), &Some(TimestampFormat::Unix));
    }

    #[test]
    fn deserialize_legacy_config() {
        for string in [r#""foo""#, r#"{ "codec": "foo" }"#] {
            let adapter = serde_json::from_str::<
                EncodingConfigAdapter<EncodingConfig<FooLegacyEncoding>, FooMigrator>,
            >(string)
            .unwrap();

            let serializer = adapter.config();

            assert!(matches!(serializer, SerializerConfig::Json));
        }
    }

    #[test]
    fn deserialize_legacy_config_with_framing() {
        for string in [
            r#"{ "encoding": "foo" }"#,
            r#"{ "encoding": { "codec": "foo" } }"#,
        ] {
            let adapter = serde_json::from_str::<
                EncodingConfigWithFramingAdapter<
                    EncodingConfig<FooLegacyEncoding>,
                    FooWithFramingMigrator,
                >,
            >(string)
            .unwrap();

            let (framing, serializer) = adapter.config();

            assert!(matches!(framing, Some(FramingConfig::NewlineDelimited)));
            assert!(matches!(serializer, SerializerConfig::Json));
        }
    }

    #[test]
    fn deserialize_legacy_config_with_framing_override() {
        for string in [
            r#"{ "framing": { "method": "bytes" }, "encoding": "foo" }"#,
            r#"{ "framing": { "method": "bytes" }, "encoding": { "codec": "foo" } }"#,
        ] {
            let adapter = serde_json::from_str::<
                EncodingConfigWithFramingAdapter<
                    EncodingConfig<FooLegacyEncoding>,
                    FooWithFramingMigrator,
                >,
            >(string)
            .unwrap();

            let (framing, serializer) = adapter.config();

            assert!(matches!(framing, Some(FramingConfig::Bytes)));
            assert!(matches!(serializer, SerializerConfig::Json));
        }
    }

    #[test]
    fn serialize_encoding_with_transformation() {
        let string = r#"{"codec":"raw_message","only_fields":["a.b[0]"],"except_fields":["ignore_me"],"timestamp_format":"unix"}"#;

        let adapter = serde_json::from_str::<
            EncodingConfigAdapter<EncodingConfig<FooLegacyEncoding>, FooMigrator>,
        >(string)
        .unwrap();

        let serialized = serde_json::to_string(&adapter).unwrap();

        assert_eq!(string, serialized);
    }

    #[test]
    fn serialize_encoding_with_framing_and_transformation() {
        let string = r#"{"framing":{"method":"character_delimited","character_delimited":{"delimiter":","}},"encoding":{"codec":"raw_message","only_fields":["a.b[0]"],"except_fields":["ignore_me"],"timestamp_format":"unix"}}"#;

        let adapter = serde_json::from_str::<
            EncodingConfigWithFramingAdapter<
                EncodingConfig<FooLegacyEncoding>,
                FooWithFramingMigrator,
            >,
        >(string)
        .unwrap();

        let serialized = serde_json::to_string(&adapter).unwrap();

        assert_eq!(string, serialized);
    }
}
