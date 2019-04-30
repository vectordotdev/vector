use super::util::SinkExt;
use crate::buffers::Acker;
use crate::sinks::encoders::{default_string_encoder, EncoderConfig};
use futures::{future, Sink};
use serde::{Deserialize, Serialize};
use tokio::codec::{FramedWrite, LinesCodec};
use tokio::io;

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Stdout,
    Stderr,
}

impl Default for Target {
    fn default() -> Self {
        Target::Stdout
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ConsoleSinkConfig {
    #[serde(default)]
    pub target: Target,
    #[serde(default = "default_string_encoder")]
    pub encoder: Box<dyn EncoderConfig>,
}

#[typetag::serde(name = "console")]
impl crate::topology::config::SinkConfig for ConsoleSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let encoder = self.encoder.build();

        let output: Box<dyn io::AsyncWrite + Send> = match self.target {
            Target::Stdout => Box::new(io::stdout()),
            Target::Stderr => Box::new(io::stderr()),
        };

        let sink = FramedWrite::new(output, LinesCodec::new())
            .stream_ack(acker)
            .sink_map_err(|_| ())
            .with(move |record| Ok(String::from_utf8_lossy(&encoder.encode(record)).into_owned()));

        Ok((Box::new(sink), Box::new(future::ok(()))))
    }
}
