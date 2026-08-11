use futures::{FutureExt, future};
use tokio::io;
use vector_lib::{
    codecs::{
        JsonSerializerConfig,
        encoding::{Framer, FramingConfig},
    },
    configurable::configurable_component,
};

use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType, Transformer},
    config::{
        AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext, ValidatedSink,
    },
    sinks::{Healthcheck, VectorSink, console::sink::WriterSink},
};

/// The [standard stream][standard_streams] to write to.
///
/// [standard_streams]: https://en.wikipedia.org/wiki/Standard_streams
#[configurable_component]
#[derive(Clone, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// Write output to [STDOUT][stdout].
    ///
    /// [stdout]: https://en.wikipedia.org/wiki/Standard_streams#Standard_output_(stdout)
    #[default]
    Stdout,

    /// Write output to [STDERR][stderr].
    ///
    /// [stderr]: https://en.wikipedia.org/wiki/Standard_streams#Standard_error_(stderr)
    Stderr,
}

/// Configuration for the `console` sink.
#[configurable_component(sink(
    "console",
    "Display observability events in the console, which can be useful for debugging purposes."
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ConsoleSinkConfig {
    #[configurable(derived)]
    #[serde(default = "default_target")]
    pub target: Target,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::is_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

const fn default_target() -> Target {
    Target::Stdout
}

impl GenerateConfig for ConsoleSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_json::to_value(Self {
            target: Target::Stdout,
            encoding: (None::<FramingConfig>, JsonSerializerConfig::default()).into(),
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "console")]
impl SinkConfig for ConsoleSinkConfig {
    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedConsoleSink {
    transformer: Transformer,
}

#[async_trait::async_trait]
impl ValidatedSink for ConsoleSinkConfig {
    type Validated = ValidatedConsoleSink;

    fn validate(&self) -> crate::Result<ValidatedConsoleSink> {
        let transformer = self.encoding.transformer();
        Ok(ValidatedConsoleSink { transformer })
    }

    async fn build(
        &self,
        validated: &ValidatedConsoleSink,
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let ValidatedConsoleSink { transformer } = validated;
        let (framer, serializer) = self.encoding.build(SinkType::StreamBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let sink: VectorSink = match self.target {
            Target::Stdout => VectorSink::from_event_streamsink(WriterSink {
                output: io::stdout(),
                transformer: transformer.clone(),
                encoder,
            }),
            Target::Stderr => VectorSink::from_event_streamsink(WriterSink {
                output: io::stderr(),
                transformer: transformer.clone(),
                encoder,
            }),
        };

        Ok((sink, future::ok(()).boxed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_lib::codecs::encoding::{
        ProtobufSerializerConfig, ProtobufSerializerOptions, SerializerConfig,
    };

    use crate::config::ValidatedSink;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ConsoleSinkConfig>();
    }

    #[test]
    fn validate_produces_usable_state() {
        let config = ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: (None::<FramingConfig>, JsonSerializerConfig::default()).into(),
            acknowledgements: Default::default(),
        };
        let _validated = config.validate().expect("validation should succeed");
        // Serializer construction is deferred to `build`; validation retains the
        // transformer so `build` can construct the encoder.
        assert!(matches!(
            config.encoding.config().1,
            SerializerConfig::Json(_)
        ));
    }

    #[test]
    fn validate_does_not_build_file_reading_serializers() {
        // A protobuf codec pointing at a nonexistent descriptor file must still
        // validate without error: serializer construction (which reads the file)
        // is deferred to `build`, so `vector validate --no-environment` never
        // touches the descriptor file.
        let config = ConsoleSinkConfig {
            target: Target::Stdout,
            encoding: (
                None::<FramingConfig>,
                ProtobufSerializerConfig {
                    protobuf: ProtobufSerializerOptions {
                        desc_file: "/nonexistent/descriptor.desc".into(),
                        message_type: "package.Message".into(),
                        use_json_names: false,
                    },
                },
            )
                .into(),
            acknowledgements: Default::default(),
        };
        let _validated = config.validate().expect("validation should succeed");
        assert!(matches!(
            config.encoding.config().1,
            SerializerConfig::Protobuf(_)
        ));
    }
}
