use std::io;

use serde::{Deserialize, Serialize};
use vector_core::{config::log_schema, event::Event};

use super::Encoder;

static DEFAULT_TEXT_ENCODER: StandardTextEncoding = StandardTextEncoding;
static DEFAULT_JSON_ENCODER: StandardJsonEncoding = StandardJsonEncoding;

/// A standardized set of encodings with common sense behavior.
///
/// Each encoding utilizes a specific default set of behavior.  For example, the standard JSON
/// encoder will encode the entire event, while the standard text encoder will only encode the
/// `message` field of an event, or fail if passed a metric.
///
/// These encodings are meant to cover the most common use cases, so if there is a need for
/// specialization, you should prefer to use your own encoding enum with suitable implementations of
/// the [`Encoder`] trait.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardEncodings {
    Text,
    Ndjson,
}

impl Encoder for StandardEncodings {
    fn encode_event(&self, event: Event, writer: &mut dyn io::Write) -> io::Result<()> {
        match self {
            StandardEncodings::Text => DEFAULT_TEXT_ENCODER.encode_event(event, writer),
            StandardEncodings::Ndjson => DEFAULT_JSON_ENCODER.encode_event(event, writer),
        }
    }
}

/// Standard implementation for encoding events as JSON.
///
/// All event types will be serialized to JSON, without pretty printing.  Uses
/// [`serde_json::to_writer`] under the hood, so all caveats mentioned therein apply here.
///
/// Each event is delimited with a newline character.
pub struct StandardJsonEncoding;

impl Encoder for StandardJsonEncoding {
    fn encode_event(&self, event: Event, mut writer: &mut dyn io::Write) -> io::Result<()> {
        match event {
            Event::Log(log) => serde_json::to_writer(&mut writer, &log)?,
            Event::Metric(metric) => serde_json::to_writer(&mut writer, &metric)?,
        }
        writer.write_all(b"\n")
    }
}

/// Standard implementation for encoding events as text.
///
/// If given a log event, the value used in the field matching the global lob schema's "message" key
/// will be written out, otherwise an empty string will be written.  If anything other than a log
/// event is given, the encoder will panic.
///
/// Each event is delimited with a newline character.
pub struct StandardTextEncoding;

impl Encoder for StandardTextEncoding {
    fn encode_event(&self, event: Event, writer: &mut dyn io::Write) -> io::Result<()> {
        match event {
            Event::Log(log) => {
                let message = log
                    .get(log_schema().message_key())
                    .map(|v| v.as_bytes())
                    .unwrap_or_default();
                let _ = writer.write_all(&message[..]);
                writer.write_all(b"\n")
            }
            _ => panic!("standard text encoding cannot be used for anything other than logs"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use chrono::{SecondsFormat, Utc};
    use vector_core::{
        config::log_schema,
        event::{Event, Metric, MetricKind, MetricValue},
    };

    use super::StandardEncodings;
    use crate::sinks::util::encoding::Encoder;

    fn encode_event(event: Event, encoding: StandardEncodings) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        let result = encoding.encode_event(event, &mut buf);
        result.map(|()| buf)
    }

    #[test]
    fn test_standard_text() {
        let encoding = StandardEncodings::Text;

        let message = "log event";
        let log_event = Event::from(message.to_string());

        let result = encode_event(log_event, encoding).expect("should not have failed");
        let encoded = std::str::from_utf8(&result).expect("result should be valid UTF-8");

        let expected = format!("{}\n", message);
        assert_eq!(expected, encoded);
    }

    #[test]
    #[should_panic]
    fn test_standard_text_panics_with_metric_event() {
        let encoding = StandardEncodings::Text;

        let metric_event = Metric::new(
            "namespace",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.23 },
        )
        .into();

        let _result = encode_event(metric_event, encoding);
    }

    #[test]
    fn test_standard_json() {
        let msg_key = log_schema().message_key();
        let ts_key = log_schema().timestamp_key();
        let now = Utc::now();
        let encoding = StandardEncodings::Ndjson;

        let message = "log event";
        let mut log_event = Event::from(message.to_string());
        log_event.as_mut_log().insert(ts_key, now);

        let result = encode_event(log_event, encoding).expect("should not have failed");
        let encoded = std::str::from_utf8(&result).expect("result should be valid UTF-8");

        // We have to hard-code the transformation of the timestamp here, as `chrono::DateTime`
        // uses a more timezone-explicit format in its `Display` implementation, while its
        // `Serialize` implementation uses RFC3339.
        let expected = format!(
            "{{\"{}\":\"log event\",\"{}\":\"{}\"}}\n",
            msg_key,
            ts_key,
            now.to_rfc3339_opts(SecondsFormat::Nanos, true)
        );
        assert_eq!(expected, encoded);
    }
}
