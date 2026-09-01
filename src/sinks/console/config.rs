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
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
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
    #[serde(default = "default_target")]
    pub target: Target,

    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,

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

#[async_trait::async_trait]
impl ValidatedSink for ConsoleSinkConfig {
    type Validated = ();

    fn validate(&self) -> crate::Result<()> {
        Ok(())
    }

    async fn build(
        &self,
        _validated: &(),
        _cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let transformer = self.encoding.transformer();
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

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ConsoleSinkConfig>();
    }
}
