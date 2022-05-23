use codecs::{
    encoding::{Framer, Serializer},
    LengthDelimitedEncoder, NewlineDelimitedEncoder,
};
use futures::{future, FutureExt};
use serde::{Deserialize, Serialize};
use tokio::io;

use crate::{
    codecs::Encoder,
    config::{AcknowledgementsConfig, GenerateConfig, Input, SinkConfig, SinkContext},
    sinks::{
        console::sink::WriterSink,
        util::encoding::{
            EncodingConfig, EncodingConfigWithFramingAdapter, StandardEncodings,
            StandardEncodingsWithFramingMigrator,
        },
        Healthcheck, VectorSink,
    },
};

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
    pub encoding: EncodingConfigWithFramingAdapter<
        EncodingConfig<StandardEncodings>,
        StandardEncodingsWithFramingMigrator,
    >,
}

impl GenerateConfig for ConsoleSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            target: Target::Stdout,
            encoding: EncodingConfig::from(StandardEncodings::Json).into(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "console")]
impl SinkConfig for ConsoleSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let transformer = self.encoding.transformer();
        let (framer, serializer) = self.encoding.encoding();
        let framer = match (framer, &serializer) {
            (Some(framer), _) => framer,
            (
                None,
                Serializer::Text(_)
                | Serializer::Json(_)
                | Serializer::Logfmt(_)
                | Serializer::NativeJson(_)
                | Serializer::RawMessage(_),
            ) => NewlineDelimitedEncoder::new().into(),
            (None, Serializer::Native(_)) => LengthDelimitedEncoder::new().into(),
        };
        let encoder = Encoder::<Framer>::new(framer, serializer);

        let sink: VectorSink = match self.target {
            Target::Stdout => VectorSink::from_event_streamsink(WriterSink {
                acker: cx.acker(),
                output: io::stdout(),
                transformer,
                encoder,
            }),
            Target::Stderr => VectorSink::from_event_streamsink(WriterSink {
                acker: cx.acker(),
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
        None
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
