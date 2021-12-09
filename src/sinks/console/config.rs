use crate::config::{DataType, GenerateConfig, SinkConfig, SinkContext};
use crate::sinks::console::sink::WriterSink;
use crate::sinks::util::encoding::{EncodingConfig, StandardEncodings};
use crate::sinks::{Healthcheck, VectorSink};
use async_trait::async_trait;
use futures::{future, FutureExt};
use serde::{Deserialize, Serialize};
use tokio::io;

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

        let output: Box<dyn io::AsyncWrite + Send + Sync + Unpin> = match self.target {
            Target::Stdout => Box::new(io::stdout()),
            Target::Stderr => Box::new(io::stderr()),
        };

        let sink = WriterSink {
            acker: cx.acker(),
            output,
            encoding,
        };

        Ok((VectorSink::Stream(Box::new(sink)), future::ok(()).boxed()))
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
