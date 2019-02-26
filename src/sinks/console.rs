use crate::record::Record;
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
}

#[typetag::serde(name = "console")]
impl crate::topology::config::SinkConfig for ConsoleSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let output: Box<dyn io::AsyncWrite + Send> = match self.target {
            Target::Stdout => Box::new(io::stdout()),
            Target::Stderr => Box::new(io::stderr()),
        };

        let sink = FramedWrite::new(output, LinesCodec::new())
            .sink_map_err(|_| ())
            .with(|record: Record| Ok(record.line));

        Ok((Box::new(sink), Box::new(future::ok(()))))
    }
}
