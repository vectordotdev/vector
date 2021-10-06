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
    Json,
    Ndjson,
}

impl StandardEncodings {
    fn batch_pre_hook(self, writer: &mut dyn io::Write) -> io::Result<usize> {
        match self {
            StandardEncodings::Json => {
                let buf = &[b'['];
                writer.write_all(buf).map(|()| buf.len())
            }
            _ => Ok(0),
        }
    }

    fn batch_post_hook(self, writer: &mut dyn io::Write) -> io::Result<usize> {
        match self {
            StandardEncodings::Json => {
                let buf = &[b']'];
                writer.write_all(buf).map(|()| buf.len())
            }
            _ => Ok(0),
        }
    }

    const fn batch_delimiter(&self) -> &[u8] {
        match self {
            StandardEncodings::Text | StandardEncodings::Ndjson => &[b'\n'],
            StandardEncodings::Json => &[b','],
        }
    }
}

impl Encoder<Event> for StandardEncodings {
    fn encode_input(&self, input: Event, writer: &mut dyn io::Write) -> io::Result<usize> {
        match self {
            StandardEncodings::Text => DEFAULT_TEXT_ENCODER.encode_input(input, writer),
            StandardEncodings::Json => DEFAULT_JSON_ENCODER.encode_input(input, writer),
            StandardEncodings::Ndjson => DEFAULT_JSON_ENCODER.encode_input(input, writer),
        }
    }
}

impl Encoder<Vec<Event>> for StandardEncodings {
    fn encode_input(&self, input: Vec<Event>, writer: &mut dyn io::Write) -> io::Result<usize> {
        let mut written = 0;
        let delimiter = self.batch_delimiter();

        let n = self.batch_pre_hook(writer)?;
        written += n;

        let last = input.len();
        for (i, event) in input.into_iter().enumerate() {
            let n = self.encode_input(event, writer)?;
            written += n;
            if i != last {
                let _ = writer.write_all(delimiter)?;
                written += delimiter.len();
            }
        }

        let n = self.batch_post_hook(writer)?;
        written += n;

        Ok(written)
    }
}

/// Standard implementation for encoding events as JSON.
///
/// All event types will be serialized to JSON, without pretty printing.  Uses
/// [`serde_json::to_writer`] under the hood, so all caveats mentioned therein apply here.
pub struct StandardJsonEncoding;

impl Encoder<Event> for StandardJsonEncoding {
    fn encode_input(&self, event: Event, writer: &mut dyn io::Write) -> io::Result<usize> {
        match event {
            Event::Log(log) => as_tracked_write(writer, &log, |writer, item| {
                serde_json::to_writer(writer, item)
            }),
            Event::Metric(metric) => as_tracked_write(writer, &metric, |writer, item| {
                serde_json::to_writer(writer, item)
            }),
        }
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

impl Encoder<Event> for StandardTextEncoding {
    fn encode_input(&self, event: Event, writer: &mut dyn io::Write) -> io::Result<usize> {
        match event {
            Event::Log(log) => {
                let message = log
                    .get(log_schema().message_key())
                    .map(|v| v.as_bytes())
                    .unwrap_or_default();
                writer.write_all(&message[..]).map(|()| message.len())
            }
            Event::Metric(metric) => {
                let message = metric.to_string().into_bytes();
                writer.write_all(&message).map(|()| message.len())
            }
        }
    }
}

fn as_tracked_write<F, I, E>(inner: &mut dyn io::Write, input: I, f: F) -> io::Result<usize>
where
    F: FnOnce(&mut dyn io::Write, I) -> Result<(), E>,
    E: Into<io::Error> + 'static,
{
    struct Tracked<'inner> {
        count: usize,
        inner: &'inner mut dyn io::Write,
    }

    impl<'inner> io::Write for Tracked<'inner> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let n = self.inner.write(buf)?;
            self.count += n;
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    let mut tracked = Tracked { count: 0, inner };
    let _ = f(&mut tracked, input).map_err(|e| e.into())?;
    Ok(tracked.count)
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
        let result = encoding.encode_input(event, &mut buf);
        result.map(|_| buf)
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
        let encoding = StandardEncodings::Json;

        let message = "log event";
        let mut log_event = Event::from(message.to_string());
        log_event.as_mut_log().insert(ts_key, now);

        let result = encode_event(log_event, encoding).expect("should not have failed");
        let encoded = std::str::from_utf8(&result).expect("result should be valid UTF-8");

        // We have to hard-code the transformation of the timestamp here, as `chrono::DateTime`
        // uses a more timezone-explicit format in its `Display` implementation, while its
        // `Serialize` implementation uses RFC3339.
        let expected = format!(
            "{{\"{}\":\"log event\",\"{}\":\"{}\"}}",
            msg_key,
            ts_key,
            now.to_rfc3339_opts(SecondsFormat::AutoSi, true)
        );
        assert_eq!(expected, encoded);
    }

    #[test]
    fn test_standard_ndjson() {
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
            now.to_rfc3339_opts(SecondsFormat::AutoSi, true)
        );
        assert_eq!(expected, encoded);
    }
}
