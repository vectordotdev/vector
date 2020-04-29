use crate::{
    event::{self, Event},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        StreamSink,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use async_trait::async_trait;
use futures::pin_mut;
use futures::stream::{Stream, StreamExt};
use futures01::future;
use serde::{Deserialize, Serialize};
use tokio::io::{self, AsyncWriteExt};

use super::streaming_sink::{self, StreamingSink};

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
    pub encoding: EncodingConfig<Encoding>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new_without_default::<ConsoleSinkConfig>("console")
}

#[typetag::serde(name = "console")]
impl SinkConfig for ConsoleSinkConfig {
    fn build(&self, mut cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let encoding = self.encoding.clone();

        let output: Box<dyn io::AsyncWrite + Send + Sync + Unpin> = match self.target {
            Target::Stdout => Box::new(io::stdout()),
            Target::Stderr => Box::new(io::stderr()),
        };

        let sink = WriterSink { output, encoding };
        let sink = streaming_sink::compat::adapt_to_topology(&mut cx, sink);
        let sink = StreamSink::new(sink, cx.acker());

        Ok((Box::new(sink), Box::new(future::ok(()))))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "console"
    }
}

fn encode_event(
    mut event: Event,
    encoding: &EncodingConfig<Encoding>,
) -> Result<String, serde_json::Error> {
    encoding.apply_rules(&mut event);
    match event {
        Event::Log(log) => match encoding.codec() {
            Encoding::Json => serde_json::to_string(&log),
            Encoding::Text => {
                let s = log
                    .get(&event::log_schema().message_key())
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into());
                Ok(s)
            }
        },
        Event::Metric(metric) => serde_json::to_string(&metric),
    }
}

async fn write_event_to_output(
    mut output: impl io::AsyncWrite + Send + Unpin,
    event: Event,
    encoding: &EncodingConfig<Encoding>,
) -> Result<(), std::io::Error> {
    let mut buf =
        encode_event(event, encoding).map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    buf.push('\n');
    output.write_all(buf.as_bytes()).await?;
    Ok(())
}

struct WriterSink {
    output: Box<dyn io::AsyncWrite + Send + Sync + Unpin>,
    encoding: EncodingConfig<Encoding>,
}

#[async_trait]
impl StreamingSink for WriterSink {
    async fn run(
        &mut self,
        input: impl Stream<Item = Event> + Send + Sync + 'static,
    ) -> crate::Result<()> {
        let output = &mut self.output;
        pin_mut!(output);
        pin_mut!(input);
        while let Some(event) = input.next().await {
            write_event_to_output(&mut output, event, &self.encoding).await?
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{encode_event, Encoding, EncodingConfig};
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use crate::event::{Event, Value};
    use chrono::{offset::TimeZone, Utc};

    #[test]
    fn encodes_raw_logs() {
        let event = Event::from("foo");
        assert_eq!(
            "foo",
            encode_event(event, &EncodingConfig::from(Encoding::Text)).unwrap()
        );
    }

    #[test]
    fn encodes_log_events() {
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();
        log.insert("x", Value::from("23"));
        log.insert("z", Value::from(25));
        log.insert("a", Value::from("0"));

        let encoded = encode_event(event, &EncodingConfig::from(Encoding::Json));
        let expected = r#"{"a":"0","x":"23","z":25}"#;
        assert_eq!(encoded.unwrap(), expected);
    }

    #[test]
    fn encodes_counter() {
        let event = Event::Metric(Metric {
            name: "foos".into(),
            timestamp: Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)),
            tags: Some(
                vec![
                    ("key2".to_owned(), "value2".to_owned()),
                    ("key1".to_owned(), "value1".to_owned()),
                    ("Key3".to_owned(), "Value3".to_owned()),
                ]
                .into_iter()
                .collect(),
            ),
            kind: MetricKind::Incremental,
            value: MetricValue::Counter { value: 100.0 },
        });
        assert_eq!(
            r#"{"name":"foos","timestamp":"2018-11-14T08:09:10.000000011Z","tags":{"Key3":"Value3","key1":"value1","key2":"value2"},"kind":"incremental","counter":{"value":100.0}}"#,
            encode_event(event, &EncodingConfig::from(Encoding::Text)).unwrap()
        );
    }

    #[test]
    fn encodes_set() {
        let event = Event::Metric(Metric {
            name: "users".into(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["bob".into()].into_iter().collect(),
            },
        });
        assert_eq!(
            r#"{"name":"users","timestamp":null,"tags":null,"kind":"incremental","set":{"values":["bob"]}}"#,
            encode_event(event, &EncodingConfig::from(Encoding::Text)).unwrap()
        );
    }

    #[test]
    fn encodes_histogram_without_timestamp() {
        let event = Event::Metric(Metric {
            name: "glork".into(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![10.0],
                sample_rates: vec![1],
            },
        });
        assert_eq!(
            r#"{"name":"glork","timestamp":null,"tags":null,"kind":"incremental","distribution":{"values":[10.0],"sample_rates":[1]}}"#,
            encode_event(event, &EncodingConfig::from(Encoding::Text)).unwrap()
        );
    }
}
