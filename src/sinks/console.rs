use crate::{
    buffers::Acker,
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    internal_events::{ConsoleEventProcessed, ConsoleFieldNotFound},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        StreamSink,
    },
};
use async_trait::async_trait;
use futures::{
    future,
    stream::{BoxStream, StreamExt},
    FutureExt,
};
use serde::{Deserialize, Serialize};

use tokio::io::{self, AsyncWriteExt};

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
    pub encoding: EncodingConfig<Encoding>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new::<ConsoleSinkConfig>("console")
}

impl GenerateConfig for ConsoleSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            target: Target::Stdout,
            encoding: Encoding::Json.into(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "console")]
impl SinkConfig for ConsoleSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
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

        Ok((
            super::VectorSink::Stream(Box::new(sink)),
            future::ok(()).boxed(),
        ))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn sink_type(&self) -> &'static str {
        "console"
    }
}

fn encode_event(mut event: Event, encoding: &EncodingConfig<Encoding>) -> Option<String> {
    encoding.apply_rules(&mut event);
    match event {
        Event::Log(log) => match encoding.codec() {
            Encoding::Json => serde_json::to_string(&log)
                .map_err(|error| {
                    error!(message = "Error encoding json.", %error);
                })
                .ok(),
            Encoding::Text => {
                let field = crate::config::log_schema().message_key();
                match log.get(field) {
                    Some(v) => Some(v.to_string_lossy()),
                    None => {
                        emit!(ConsoleFieldNotFound {
                            missing_field: field,
                        });
                        None
                    }
                }
            }
        },
        Event::Metric(metric) => match encoding.codec() {
            Encoding::Json => serde_json::to_string(&metric)
                .map_err(|error| {
                    error!(message = "Error encoding json.", %error);
                })
                .ok(),
            Encoding::Text => Some(format!("{}", metric)),
        },
    }
}

struct WriterSink {
    acker: Acker,
    output: Box<dyn io::AsyncWrite + Send + Sync + Unpin>,
    encoding: EncodingConfig<Encoding>,
}

#[async_trait]
impl StreamSink for WriterSink {
    async fn run(&mut self, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            self.acker.ack(1);
            if let Some(mut buf) = encode_event(event, &self.encoding) {
                buf.push('\n');
                if let Err(error) = self.output.write_all(buf.as_bytes()).await {
                    // Error when writing to stdout/stderr is likely irrecoverable,
                    // so stop the sink.
                    error!(message = "Error writing to output. Stopping sink.", %error);
                    return Err(());
                }

                emit!(ConsoleEventProcessed {
                    byte_size: buf.len(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{encode_event, ConsoleSinkConfig, Encoding, EncodingConfig};
    use crate::event::metric::{Metric, MetricKind, MetricValue, StatisticKind};
    use crate::event::{Event, Value};
    use chrono::{offset::TimeZone, Utc};
    use pretty_assertions::assert_eq;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<ConsoleSinkConfig>();
    }

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
        let event = Event::Metric(Metric::new(
            "foos".into(),
            Some("vector".into()),
            Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)),
            Some(
                vec![
                    ("key2".to_owned(), "value2".to_owned()),
                    ("key1".to_owned(), "value1".to_owned()),
                    ("Key3".to_owned(), "Value3".to_owned()),
                ]
                .into_iter()
                .collect(),
            ),
            MetricKind::Incremental,
            MetricValue::Counter { value: 100.0 },
        ));
        assert_eq!(
            r#"{"name":"foos","namespace":"vector","tags":{"Key3":"Value3","key1":"value1","key2":"value2"},"timestamp":"2018-11-14T08:09:10.000000011Z","kind":"incremental","counter":{"value":100.0}}"#,
            encode_event(event, &EncodingConfig::from(Encoding::Json)).unwrap()
        );
    }

    #[test]
    fn encodes_set() {
        let event = Event::Metric(Metric::new(
            "users".into(),
            None,
            None,
            None,
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["bob".into()].into_iter().collect(),
            },
        ));
        assert_eq!(
            r#"{"name":"users","kind":"incremental","set":{"values":["bob"]}}"#,
            encode_event(event, &EncodingConfig::from(Encoding::Json)).unwrap()
        );
    }

    #[test]
    fn encodes_histogram_without_timestamp() {
        let event = Event::Metric(Metric::new(
            "glork".into(),
            None,
            None,
            None,
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: crate::samples![10.0 => 1],
                statistic: StatisticKind::Histogram,
            },
        ));
        assert_eq!(
            r#"{"name":"glork","kind":"incremental","distribution":{"samples":[{"value":10.0,"rate":1}],"statistic":"histogram"}}"#,
            encode_event(event, &EncodingConfig::from(Encoding::Json)).unwrap()
        );
    }

    #[test]
    fn encodes_metric_text() {
        let event = Event::Metric(Metric::new(
            "users".into(),
            None,
            None,
            None,
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["bob".into()].into_iter().collect(),
            },
        ));
        assert_eq!(
            "users{} + bob",
            encode_event(event, &EncodingConfig::from(Encoding::Text)).unwrap()
        );
    }
}
