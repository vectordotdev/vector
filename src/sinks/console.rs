use super::util::SinkExt;
use crate::buffers::Acker;
use crate::event::{self, Event};
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
    pub encoding: Option<Encoding>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

#[typetag::serde(name = "console")]
impl crate::topology::config::SinkConfig for ConsoleSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let encoding = self.encoding.clone();

        let output: Box<dyn io::AsyncWrite + Send> = match self.target {
            Target::Stdout => Box::new(io::stdout()),
            Target::Stderr => Box::new(io::stderr()),
        };

        let sink = FramedWrite::new(output, LinesCodec::new())
            .stream_ack(acker)
            .sink_map_err(|_| ())
            .with(move |event| encode_event(event, &encoding));

        Ok((Box::new(sink), Box::new(future::ok(()))))
    }
}

fn encode_event(event: Event, encoding: &Option<Encoding>) -> Result<String, ()> {
    let log = event.into_log();

    if (log.is_structured() && encoding != &Some(Encoding::Text))
        || encoding == &Some(Encoding::Json)
    {
        let bytes =
            serde_json::to_vec(&log.all_fields()).map_err(|e| panic!("Error encoding: {}", e))?;
        String::from_utf8(bytes).map_err(|e| panic!("Unable to convert json to utf8: {}", e))
    } else {
        let s = log[&event::MESSAGE].to_string_lossy();
        Ok(s)
    }
}
