use vector_config::configurable_component;

use super::{Encoder, EncoderKind, Transformer};
use crate::encoding::{
    CharacterDelimitedEncoder, Framer, FramingConfig, LengthDelimitedEncoder,
    NewlineDelimitedEncoder, Serializer, SerializerConfig,
};

#[cfg(feature = "opentelemetry")]
use crate::encoding::BytesEncoder;

/// Encoding configuration.
#[configurable_component]
#[derive(Clone, Debug)]
/// Configures how events are encoded into raw bytes.
/// The selected encoding also determines which input types (logs, metrics, traces) are supported.
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

    /// Build the `Serializer` for this config.
    pub fn build(&self) -> vector_common::Result<Serializer> {
        self.encoding.build()
    }

    /// Validate that the configured serializer can be built.
    ///
    /// Builds the serializer and discards it, surfacing unbuildable encodings
    /// during pure config validation instead of at build time.
    ///
    /// The protobuf codec is skipped: building it reads the descriptor set
    /// from `desc_file` on disk, and pure validation must stay
    /// filesystem-free (it runs under `vector validate --no-environment`).
    /// A protobuf descriptor that can't be loaded is caught by the
    /// environment-dependent `build()` phase instead.
    pub fn validate(&self) -> vector_common::Result<()> {
        match self.config() {
            SerializerConfig::Protobuf(_) => Ok(()),
            _ => self
                .build()
                .map(|_| ())
                .map_err(|error| format!("failed to build encoding serializer: {error}").into()),
        }
    }
}

impl<T> From<T> for EncodingConfig
where
    T: Into<SerializerConfig>,
{
    fn from(encoding: T) -> Self {
        Self {
            encoding: encoding.into(),
            transformer: Default::default(),
        }
    }
}

/// Encoding configuration.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfigWithFraming {
    framing: Option<FramingConfig>,

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

    /// Build the `Framer` and `Serializer` for this config.
    pub fn build(&self, sink_type: SinkType) -> vector_common::Result<(Framer, Serializer)> {
        let framer = self.framing.as_ref().map(|framing| framing.build());
        let serializer = self.encoding.build()?;

        let framer = match (framer, &serializer) {
            (Some(framer), _) => framer,
            (None, Serializer::Json(_)) => match sink_type {
                SinkType::StreamBased => NewlineDelimitedEncoder::default().into(),
                SinkType::MessageBased => CharacterDelimitedEncoder::new(b',').into(),
            },
            (None, Serializer::Avro(_) | Serializer::Native(_)) => {
                LengthDelimitedEncoder::default().into()
            }
            (None, Serializer::Gelf(_)) => {
                // Graylog/GELF always uses null byte delimiter on TCP, see
                // https://github.com/Graylog2/graylog2-server/issues/1240
                CharacterDelimitedEncoder::new(0).into()
            }
            (None, Serializer::Protobuf(_)) => {
                // Protobuf uses length-delimited messages, see:
                // https://developers.google.com/protocol-buffers/docs/techniques#streaming
                LengthDelimitedEncoder::default().into()
            }
            (
                None,
                Serializer::Cef(_)
                | Serializer::Csv(_)
                | Serializer::Logfmt(_)
                | Serializer::NativeJson(_)
                | Serializer::RawMessage(_)
                | Serializer::Text(_),
            ) => NewlineDelimitedEncoder::default().into(),
            #[cfg(feature = "syslog")]
            (None, Serializer::Syslog(_)) => NewlineDelimitedEncoder::default().into(),
            #[cfg(feature = "opentelemetry")]
            (None, Serializer::Otlp(_)) => BytesEncoder.into(),
        };

        Ok((framer, serializer))
    }

    /// Build the `Transformer` and `EncoderKind` for this config.
    pub fn build_encoder(
        &self,
        sink_type: SinkType,
    ) -> vector_common::Result<(Transformer, EncoderKind)> {
        let (framer, serializer) = self.build(sink_type)?;
        let encoder = EncoderKind::Framed(Box::new(Encoder::<Framer>::new(framer, serializer)));
        Ok((self.transformer(), encoder))
    }

    /// Validate that the configured serializer can be built.
    ///
    /// Delegates to [`EncodingConfig::validate`]: the serializer is built and
    /// discarded to surface unbuildable codecs during pure config validation,
    /// with the disk-bound protobuf codec assumed valid so validation stays
    /// filesystem-free (it runs under `vector validate --no-environment`).
    /// Building the framer is infallible, so there is nothing to validate on
    /// the framing side.
    pub fn validate(&self) -> vector_common::Result<()> {
        self.encoding.validate()
    }
}

/// The way a sink processes outgoing events.
pub enum SinkType {
    /// Events are sent in a continuous stream.
    StreamBased,
    /// Events are sent in a batch as a message.
    MessageBased,
}

impl<F, S> From<(Option<F>, S)> for EncodingConfigWithFraming
where
    F: Into<FramingConfig>,
    S: Into<SerializerConfig>,
{
    fn from((framing, encoding): (Option<F>, S)) -> Self {
        Self {
            framing: framing.map(Into::into),
            encoding: encoding.into().into(),
        }
    }
}

#[cfg(test)]
mod test {
    use lookup::lookup_v2::{ConfigValuePath, parse_value_path};

    use super::*;
    use crate::encoding::TimestampFormat;
    use crate::encoding::{
        AvroSerializerOptions, JsonSerializerConfig, ProtobufSerializerConfig,
        ProtobufSerializerOptions,
    };

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

        assert!(matches!(serializer, SerializerConfig::Json(_)));

        let transformer = encoding.transformer();

        assert_eq!(
            transformer.only_fields(),
            &Some(vec![ConfigValuePath(parse_value_path("a.b[0]").unwrap())])
        );
        assert_eq!(transformer.except_fields(), &Some(vec!["ignore_me".into()]));
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
        assert!(matches!(serializer, SerializerConfig::Json(_)));

        let transformer = encoding.transformer();

        assert_eq!(
            transformer.only_fields(),
            &Some(vec![ConfigValuePath(parse_value_path("a.b[0]").unwrap())])
        );
        assert_eq!(transformer.except_fields(), &Some(vec!["ignore_me".into()]));
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

        assert!(framing.is_none());
        assert!(matches!(serializer, SerializerConfig::Json(_)));

        let transformer = encoding.transformer();

        assert_eq!(
            transformer.only_fields(),
            &Some(vec![ConfigValuePath(parse_value_path("a.b[0]").unwrap())])
        );
        assert_eq!(transformer.except_fields(), &Some(vec!["ignore_me".into()]));
        assert_eq!(transformer.timestamp_format(), &Some(TimestampFormat::Unix));
    }

    #[test]
    fn validate_skips_protobuf_encoding_that_reads_disk() {
        // Building a protobuf serializer reads `desc_file` from disk; pure
        // validation must stay filesystem-free, so the codec is assumed valid
        // here and actually built (and failed) in the build phase.
        let encoding = EncodingConfig::new(
            SerializerConfig::Protobuf(ProtobufSerializerConfig {
                protobuf: ProtobufSerializerOptions {
                    desc_file: "/nonexistent/protobuf.desc".into(),
                    message_type: "package.Message".into(),
                    use_json_names: false,
                },
            }),
            Default::default(),
        );

        assert!(encoding.validate().is_ok());
    }

    #[test]
    fn validate_rejects_unbuildable_encoding() {
        // Avro's schema is inline JSON, so building it is filesystem-free and
        // pure validation catches a malformed schema.
        let encoding = EncodingConfig::new(
            SerializerConfig::Avro {
                avro: AvroSerializerOptions {
                    schema: "not a valid avro schema".into(),
                },
            },
            Default::default(),
        );

        let error = encoding.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("failed to build encoding serializer"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn validate_accepts_buildable_encoding() {
        let encoding = EncodingConfig::new(
            SerializerConfig::Json(JsonSerializerConfig::default()),
            Default::default(),
        );

        assert!(encoding.validate().is_ok());
    }

    #[test]
    fn validate_with_framing_rejects_unbuildable_encoding() {
        let encoding = EncodingConfigWithFraming::new(
            Some(FramingConfig::NewlineDelimited),
            SerializerConfig::Avro {
                avro: AvroSerializerOptions {
                    schema: "not a valid avro schema".into(),
                },
            },
            Default::default(),
        );

        let error = encoding.validate().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("failed to build encoding serializer"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn validate_with_framing_skips_protobuf_encoding_that_reads_disk() {
        let encoding = EncodingConfigWithFraming::new(
            None,
            SerializerConfig::Protobuf(ProtobufSerializerConfig {
                protobuf: ProtobufSerializerOptions {
                    desc_file: "/nonexistent/protobuf.desc".into(),
                    message_type: "package.Message".into(),
                    use_json_names: false,
                },
            }),
            Default::default(),
        );

        assert!(encoding.validate().is_ok());
    }
}
