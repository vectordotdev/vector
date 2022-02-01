use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use tokio::{io, io::AsyncWriteExt};
use vector_core::{
    buffers::Acker,
    internal_event::{BytesSent, EventsSent},
    ByteSizeOf,
};

use crate::{
    event::Event,
    internal_events::ConsoleEventProcessed,
    sinks::util::{
        encoding::{Encoder, EncodingConfig, StandardEncodings},
        StreamSink,
    },
};

pub struct WriterSink<T> {
    pub acker: Acker,
    pub output: T,
    pub encoding: EncodingConfig<StandardEncodings>,
}

#[async_trait]
impl<T> StreamSink<Event> for WriterSink<T>
where
    T: io::AsyncWrite + Send + Sync + Unpin,
{
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(event) = input.next().await {
            let event_byte_size = event.size_of();
            if let Some(mut buf) = encode_event(event, &self.encoding) {
                buf.push('\n');
                if let Err(error) = self.output.write_all(buf.as_bytes()).await {
                    // Error when writing to stdout/stderr is likely irrecoverable,
                    // so stop the sink.
                    error!(message = "Error writing to output. Stopping sink.", %error);
                    return Err(());
                }
                self.acker.ack(1);

                emit!(&ConsoleEventProcessed {
                    byte_size: buf.len(),
                });
                emit!(&EventsSent {
                    byte_size: event_byte_size,
                    count: 1,
                    output: None,
                });
                emit!(&BytesSent {
                    byte_size: buf.len(),
                    protocol: "console"
                });
            }
        }
        Ok(())
    }
}

fn encode_event(event: Event, encoding: &EncodingConfig<StandardEncodings>) -> Option<String> {
    encoding.encode_input_to_string(event).ok()
}

#[cfg(test)]
mod test {
    use chrono::{offset::TimeZone, Utc};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        event::{
            metric::{Metric, MetricKind, MetricValue, StatisticKind},
            Event, Value,
        },
        sinks::util::encoding::StandardEncodings,
    };

    #[test]
    fn encodes_raw_logs() {
        let event = Event::from("foo");
        assert_eq!(
            "foo",
            encode_event(event, &EncodingConfig::from(StandardEncodings::Text)).unwrap()
        );
    }

    #[test]
    fn encodes_log_events() {
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();
        log.insert("x", Value::from("23"));
        log.insert("z", Value::from(25));
        log.insert("a", Value::from("0"));

        let encoded = encode_event(event, &EncodingConfig::from(StandardEncodings::Json));
        let expected = r#"{"a":"0","x":"23","z":25}"#;
        assert_eq!(encoded.unwrap(), expected);
    }

    #[test]
    fn encodes_counter() {
        let event = Event::Metric(
            Metric::new(
                "foos",
                MetricKind::Incremental,
                MetricValue::Counter { value: 100.0 },
            )
            .with_namespace(Some("vector"))
            .with_tags(Some(
                vec![
                    ("key2".to_owned(), "value2".to_owned()),
                    ("key1".to_owned(), "value1".to_owned()),
                    ("Key3".to_owned(), "Value3".to_owned()),
                ]
                .into_iter()
                .collect(),
            ))
            .with_timestamp(Some(Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11))),
        );
        assert_eq!(
            r#"{"name":"foos","namespace":"vector","tags":{"Key3":"Value3","key1":"value1","key2":"value2"},"timestamp":"2018-11-14T08:09:10.000000011Z","kind":"incremental","counter":{"value":100.0}}"#,
            encode_event(event, &EncodingConfig::from(StandardEncodings::Json)).unwrap()
        );
    }

    #[test]
    fn encodes_set() {
        let event = Event::Metric(Metric::new(
            "users",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["bob".into()].into_iter().collect(),
            },
        ));
        assert_eq!(
            r#"{"name":"users","kind":"incremental","set":{"values":["bob"]}}"#,
            encode_event(event, &EncodingConfig::from(StandardEncodings::Json)).unwrap()
        );
    }

    #[test]
    fn encodes_histogram_without_timestamp() {
        let event = Event::Metric(Metric::new(
            "glork",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![10.0 => 1],
                statistic: StatisticKind::Histogram,
            },
        ));
        assert_eq!(
            r#"{"name":"glork","kind":"incremental","distribution":{"samples":[{"value":10.0,"rate":1}],"statistic":"histogram"}}"#,
            encode_event(event, &EncodingConfig::from(StandardEncodings::Json)).unwrap()
        );
    }

    #[test]
    fn encodes_metric_text() {
        let event = Event::Metric(Metric::new(
            "users",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["bob".into()].into_iter().collect(),
            },
        ));
        assert_eq!(
            "users{} + bob",
            encode_event(event, &EncodingConfig::from(StandardEncodings::Text)).unwrap()
        );
    }
}
