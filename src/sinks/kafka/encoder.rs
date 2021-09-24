use crate::config::log_schema;
use crate::event::Event;
use crate::sinks::util::encoding::Encoder;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

impl Encoder for Encoding {
    fn encode_event(&self, event: Event, writer: &mut dyn Write) -> std::io::Result<()> {
        match self {
            Encoding::Json => {
                match event {
                    Event::Log(log) => serde_json::to_writer(writer, &log)?,
                    Event::Metric(metric) => serde_json::to_writer(writer, &metric)?,
                }
                Ok(())
            }
            Encoding::Text => match event {
                Event::Log(log) => {
                    let message = log
                        .get(log_schema().message_key())
                        .map(|v| v.as_bytes().to_vec())
                        .unwrap_or_default();
                    writer.write_all(&message)
                }
                Event::Metric(metric) => writer.write_all(&metric.to_string().into_bytes()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::log_schema;
    use crate::event::{Metric, MetricKind, MetricValue};
    use std::collections::BTreeMap;

    #[test]
    fn kafka_encode_event_log_text() {
        crate::test_util::trace_init();
        let message = "hello world".to_string();
        let mut bytes = vec![];
        Encoding::Text
            .encode_event(message.clone().into(), &mut bytes)
            .unwrap();
        assert_eq!(&bytes[..], message.as_bytes());
    }

    #[test]
    fn kafka_encode_event_log_json() {
        crate::test_util::trace_init();
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        event.as_mut_log().insert("foo", "bar");

        let mut bytes = vec![];
        Encoding::Json.encode_event(event, &mut bytes).unwrap();

        let map: BTreeMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();

        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
        assert_eq!(map["foo"], "bar".to_string());
    }

    #[test]
    fn kafka_encode_event_metric_text() {
        let metric = Metric::new(
            "kafka-metric",
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.0 },
        );
        let mut bytes = vec![];
        Encoding::Text
            .encode_event(metric.clone().into(), &mut bytes)
            .unwrap();
        assert_eq!(metric.to_string(), String::from_utf8_lossy(&bytes));
    }

    #[test]
    fn kafka_encode_event_metric_json() {
        let metric = Metric::new(
            "kafka-metric",
            MetricKind::Absolute,
            MetricValue::Counter { value: 0.0 },
        );

        let mut bytes = vec![];
        Encoding::Json
            .encode_event(metric.clone().into(), &mut bytes)
            .unwrap();

        assert_eq!(
            serde_json::to_string(&metric).unwrap(),
            String::from_utf8_lossy(&bytes)
        );
    }
}
