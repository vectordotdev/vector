use super::util::SinkExt;
use crate::{
    buffers::Acker,
    event::{self, Event},
    topology::config::{DataType, SinkConfig},
};
use futures::{future, Sink};
use serde::{Deserialize, Serialize};
use tokio::{
    codec::{FramedWrite, LinesCodec},
    io,
};

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
impl SinkConfig for ConsoleSinkConfig {
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

    fn input_type(&self) -> DataType {
        DataType::Any
    }
}

fn encode_event(event: Event, encoding: &Option<Encoding>) -> Result<String, ()> {
    match event {
        Event::Log(log) => {
            if (log.is_structured() && encoding != &Some(Encoding::Text))
                || encoding == &Some(Encoding::Json)
            {
                let bytes = serde_json::to_vec(&log.unflatten())
                    .map_err(|e| panic!("Error encoding: {}", e))?;
                String::from_utf8(bytes)
                    .map_err(|e| panic!("Unable to convert json to utf8: {}", e))
            } else {
                let s = log
                    .get(&event::MESSAGE)
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into());
                Ok(s)
            }
        }
        Event::Metric(metric) => serde_json::to_string(&metric).map_err(|_| ()),
    }
}

#[cfg(test)]
mod test {
    use super::encode_event;
    use crate::{event::Metric, Event};

    #[test]
    fn encodes_raw_logs() {
        let event = Event::from("foo");
        assert_eq!(Ok("foo".to_string()), encode_event(event, &None));
    }

    #[test]
    fn encodes_metrics() {
        let event = Event::Metric(Metric::Counter {
            name: "foos".into(),
            val: 100.0,
        });
        assert_eq!(
            Ok(r#"{"type":"counter","name":"foos","val":100.0}"#.to_string()),
            encode_event(event, &None)
        );
    }
}
