use futures::{future, FutureExt};
use serde::{Deserialize, Serialize};
use tokio::io;

use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    sinks::{
        console::sink::WriterSink,
        util::{
            encoding::{EncodingConfig, StandardEncodings},
            StreamSink,
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
    pub encoding: EncodingConfig<StandardEncodings>,
}

impl GenerateConfig for ConsoleSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            target: Target::Stdout,
            encoding: StandardEncodings::Json.into(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "console")]
impl SinkConfig for ConsoleSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let encoding = self.encoding.clone();

        let sink: Box<dyn StreamSink + Send> = match self.target {
            Target::Stdout => Box::new(WriterSink {
                acker: cx.acker(),
                output: io::stdout(),
                encoding,
            }),
            Target::Stderr => Box::new(WriterSink {
                acker: cx.acker(),
                output: io::stderr(),
                encoding,
            }),
        };

        Ok((VectorSink::Stream(sink), future::ok(()).boxed()))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "console"
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
