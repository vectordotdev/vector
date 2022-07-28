use futures::{future, FutureExt};
use serde::{Deserialize, Serialize};
use tokio::io;
use vector_sdk::{
    codecs::{
        encoding::{Framer, FramingConfig},
        JsonSerializerConfig,
    },
    codecs_extra::{Encoder, EncodingConfigWithFraming, SinkType},
    config::{
        sink::{Healthcheck, SinkConfig, SinkContext},
        GenerateConfig,
    },
    core::{
        config::{AcknowledgementsConfig, Input},
        sink::VectorSink,
    },
};

use crate::sink::WriterSink;

#[derive(Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    #[derivative(Default)]
    Stdout,
    Stderr,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ConsoleSinkConfig {
    #[serde(default)]
    pub target: Target,
    #[serde(flatten)]
    pub encoding: EncodingConfigWithFraming,
    #[serde(
        default,
        deserialize_with = "vector_sdk::core::serde::bool_or_struct",
        skip_serializing_if = "vector_sdk::core::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for ConsoleSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            target: Target::Stdout,
            encoding: (None::<FramingConfig>, JsonSerializerConfig::new()).into(),
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "console")]
impl SinkConfig for ConsoleSinkConfig {
    async fn build(&self, _cx: SinkContext) -> vector_sdk::core::Result<(VectorSink, Healthcheck)> {
        let transformer = self.encoding.transformer();
        let (framer, serializer) = self.encoding.build(SinkType::StreamBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let sink: VectorSink = match self.target {
            Target::Stdout => VectorSink::from_event_streamsink(WriterSink {
                output: io::stdout(),
                transformer,
                encoder,
            }),
            Target::Stderr => VectorSink::from_event_streamsink(WriterSink {
                output: io::stderr(),
                transformer,
                encoder,
            }),
        };

        Ok((sink, future::ok(()).boxed()))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type())
    }

    fn sink_type(&self) -> &'static str {
        "console"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
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
