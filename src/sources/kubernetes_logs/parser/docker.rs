use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use snafu::{OptionExt, ResultExt, Snafu};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::lookup::{self, path, OwnedTargetPath};

use crate::sources::kubernetes_logs::transform_utils::get_message_path;
use crate::{
    config::log_schema,
    event::{self, Event, LogEvent, Value},
    internal_events::KubernetesLogsDockerFormatParseError,
    sources::kubernetes_logs::Config,
    transforms::{FunctionTransform, OutputBuffer},
};

pub const MESSAGE_KEY: &str = "log";
pub const STREAM_KEY: &str = "stream";
pub const TIMESTAMP_KEY: &str = "time";

/// Parser for the Docker log format.
///
/// Expects logs to arrive in a JSONLines format with the fields names and
/// contents specific to the implementation of the Docker `json-file` log driver.
///
/// Normalizes parsed data for consistency.
#[derive(Clone, Debug)]
pub(super) struct Docker {
    log_namespace: LogNamespace,
}

impl Docker {
    pub const fn new(log_namespace: LogNamespace) -> Self {
        Self { log_namespace }
    }
}

impl FunctionTransform for Docker {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        let log = event.as_mut_log();
        if let Err(err) = parse_json(log, self.log_namespace) {
            emit!(KubernetesLogsDockerFormatParseError { error: &err });
            return;
        }
        if let Err(err) = normalize_event(log, self.log_namespace) {
            emit!(KubernetesLogsDockerFormatParseError { error: &err });
            return;
        }
        output.push(event);
    }
}

/// Parses `message` as json object and removes it.
fn parse_json(log: &mut LogEvent, log_namespace: LogNamespace) -> Result<(), ParsingError> {
    let target_path = get_message_path(log_namespace);

    let value = log
        .remove(&target_path)
        .ok_or(ParsingError::NoMessageField)?;

    let bytes = match value {
        Value::Bytes(bytes) => bytes,
        _ => return Err(ParsingError::MessageFieldNotInBytes),
    };

    match serde_json::from_slice(bytes.as_ref()) {
        Ok(JsonValue::Object(object)) => {
            for (key, value) in object {
                match key.as_str() {
                    MESSAGE_KEY => drop(log.insert(&target_path, value)),
                    STREAM_KEY => log_namespace.insert_source_metadata(
                        Config::NAME,
                        log,
                        Some(LegacyKey::Overwrite(path!(STREAM_KEY))),
                        path!(STREAM_KEY),
                        value,
                    ),
                    TIMESTAMP_KEY => log_namespace.insert_source_metadata(
                        Config::NAME,
                        log,
                        log_schema().timestamp_key().map(LegacyKey::Overwrite),
                        path!("timestamp"),
                        value,
                    ),
                    _ => unreachable!("all json-file keys should be matched"),
                };
            }
            Ok(())
        }
        Ok(_) => Err(ParsingError::NotAnObject { message: bytes }),
        Err(err) => Err(ParsingError::InvalidJson {
            source: err,
            message: bytes,
        }),
    }
}

const DOCKER_MESSAGE_SPLIT_THRESHOLD: usize = 16 * 1024; // 16 Kib

fn normalize_event(
    log: &mut LogEvent,
    log_namespace: LogNamespace,
) -> Result<(), NormalizationError> {
    // Parse timestamp.
    let timestamp_key = match log_namespace {
        LogNamespace::Vector => Some(OwnedTargetPath::metadata(lookup::owned_value_path!(
            "kubernetes_logs",
            "timestamp"
        ))),
        LogNamespace::Legacy => log_schema()
            .timestamp_key()
            .map(|path| OwnedTargetPath::event(path.clone())),
    };

    if let Some(timestamp_key) = timestamp_key {
        let time = log.remove(&timestamp_key).context(TimeFieldMissingSnafu)?;
        let time = time
            .as_str()
            .ok_or(NormalizationError::TimeValueUnexpectedType)?;
        let time = DateTime::parse_from_rfc3339(time.as_ref()).context(TimeParsingSnafu)?;
        log_namespace.insert_source_metadata(
            Config::NAME,
            log,
            log_schema().timestamp_key().map(LegacyKey::Overwrite),
            path!("timestamp"),
            time.with_timezone(&Utc),
        );
    }

    // Parse message, remove trailing newline and detect if it's partial.
    let message_path = get_message_path(log_namespace);
    let message = log.remove(&message_path).context(LogFieldMissingSnafu)?;
    let mut message = match message {
        Value::Bytes(val) => val,
        _ => return Err(NormalizationError::LogValueUnexpectedType),
    };
    // Here we apply out heuristics to detect if message is partial.
    // Partial messages are only split in docker at the maximum message length
    // (`DOCKER_MESSAGE_SPLIT_THRESHOLD`).
    // Thus, for a message to be partial it also has to have exactly that
    // length.
    // Now, whether that message will or won't actually be partial if it has
    // exactly the max length is unknown. We consider all messages with the
    // exact length of `DOCKER_MESSAGE_SPLIT_THRESHOLD` bytes partial
    // by default, and then, if they end with newline - consider that
    // an exception and make them non-partial.
    // This is still not ideal, and can potentially be improved.
    let mut is_partial = message.len() == DOCKER_MESSAGE_SPLIT_THRESHOLD;
    if message.last().map(|&b| b as char == '\n').unwrap_or(false) {
        message.truncate(message.len() - 1);
        is_partial = false;
    };
    log.insert(&message_path, message);

    // For partial messages add a partial event indicator.
    if is_partial {
        log_namespace.insert_source_metadata(
            Config::NAME,
            log,
            Some(LegacyKey::Overwrite(path!(event::PARTIAL))),
            path!(event::PARTIAL),
            true,
        );
    }

    Ok(())
}

#[derive(Debug, Snafu)]
enum ParsingError {
    NoMessageField,
    MessageFieldNotInBytes,
    #[snafu(display(
        "Could not parse json: {} in message {:?}",
        source,
        String::from_utf8_lossy(message)
    ))]
    InvalidJson {
        source: serde_json::Error,
        message: Bytes,
    },
    #[snafu(display("Message was not an object: {:?}", String::from_utf8_lossy(message)))]
    NotAnObject {
        message: Bytes,
    },
}

#[derive(Debug, Snafu)]
enum NormalizationError {
    TimeFieldMissing,
    TimeValueUnexpectedType,
    TimeParsing { source: chrono::ParseError },
    LogFieldMissing,
    LogValueUnexpectedType,
}

#[cfg(test)]
pub mod tests {
    use super::{super::test_util, *};
    use crate::test_util::trace_init;
    use vrl::value;

    fn make_long_string(base: &str, len: usize) -> String {
        base.chars().cycle().take(len).collect()
    }

    /// Shared test cases.
    pub fn valid_cases(log_namespace: LogNamespace) -> Vec<(Bytes, Vec<Event>)> {
        vec![
            (
                Bytes::from(
                    r#"{"log": "The actual log line\n", "stream": "stderr", "time": "2016-10-05T00:00:30.082640485Z"}"#,
                ),
                vec![test_util::make_log_event(
                    value!("The actual log line"),
                    "2016-10-05T00:00:30.082640485Z",
                    "stderr",
                    false,
                    log_namespace,
                )],
            ),
            (
                Bytes::from(
                    r#"{"log": "A line without newline char at the end", "stream": "stdout", "time": "2016-10-05T00:00:30.082640485Z"}"#,
                ),
                vec![test_util::make_log_event(
                    value!("A line without newline char at the end"),
                    "2016-10-05T00:00:30.082640485Z",
                    "stdout",
                    false,
                    log_namespace,
                )],
            ),
            // Partial message due to message length.
            (
                Bytes::from(
                    [
                        r#"{"log": ""#,
                        make_long_string("partial ", 16 * 1024).as_str(),
                        r#"", "stream": "stdout", "time": "2016-10-05T00:00:30.082640485Z"}"#,
                    ]
                    .join(""),
                ),
                vec![test_util::make_log_event(
                    value!(make_long_string("partial ", 16 * 1024)),
                    "2016-10-05T00:00:30.082640485Z",
                    "stdout",
                    true,
                    log_namespace,
                )],
            ),
            // Non-partial message, because message length matches but
            // the message also ends with newline.
            (
                Bytes::from(
                    [
                        r#"{"log": ""#,
                        make_long_string("non-partial ", 16 * 1024 - 1).as_str(),
                        r"\n",
                        r#"", "stream": "stdout", "time": "2016-10-05T00:00:30.082640485Z"}"#,
                    ]
                    .join(""),
                ),
                vec![test_util::make_log_event(
                    value!(make_long_string("non-partial ", 16 * 1024 - 1)),
                    "2016-10-05T00:00:30.082640485Z",
                    "stdout",
                    false,
                    log_namespace,
                )],
            ),
        ]
    }

    pub fn invalid_cases() -> Vec<Bytes> {
        vec![
            // Empty string.
            Bytes::from(""),
            // Incomplete.
            Bytes::from("{"),
            // Random non-JSON text.
            Bytes::from("hello world"),
            // Random JSON non-object.
            Bytes::from("123"),
            // Empty JSON object.
            Bytes::from("{}"),
            // No timestamp.
            Bytes::from(r#"{"log": "Hello world", "stream": "stdout"}"#),
            // Timestamp not a string.
            Bytes::from(r#"{"log": "Hello world", "stream": "stdout", "time": 123}"#),
            // Empty timestamp.
            Bytes::from(r#"{"log": "Hello world", "stream": "stdout", "time": ""}"#),
            // Invalid timestamp.
            Bytes::from(r#"{"log": "Hello world", "stream": "stdout", "time": "qwerty"}"#),
            // No log field.
            Bytes::from(r#"{"stream": "stderr", "time": "2016-10-05T00:00:30.082640485Z"}"#),
            // Log is not a string.
            Bytes::from(
                r#"{"log": 123, "stream": "stderr", "time": "2016-10-05T00:00:30.082640485Z"}"#,
            ),
        ]
    }

    #[test]
    fn test_parsing_valid_vector_namespace() {
        trace_init();

        test_util::test_parser(
            || Docker {
                log_namespace: LogNamespace::Vector,
            },
            |bytes| Event::Log(LogEvent::from(value!(bytes))),
            valid_cases(LogNamespace::Vector),
        );
    }

    #[test]
    fn test_parsing_valid_legacy_namespace() {
        trace_init();

        test_util::test_parser(
            || Docker {
                log_namespace: LogNamespace::Legacy,
            },
            |bytes| Event::Log(LogEvent::from(bytes)),
            valid_cases(LogNamespace::Legacy),
        );
    }

    #[test]
    fn test_parsing_invalid_vector_namespace() {
        trace_init();

        let cases = invalid_cases();

        for bytes in cases {
            let mut parser = Docker::new(LogNamespace::Vector);
            let input = LogEvent::from(value!(bytes));
            let mut output = OutputBuffer::default();
            parser.transform(&mut output, input.into());

            assert!(output.is_empty(), "Expected no events: {:?}", output);
        }
    }

    #[test]
    fn test_parsing_invalid_legacy_namespace() {
        trace_init();

        let cases = invalid_cases();

        for bytes in cases {
            let mut parser = Docker::new(LogNamespace::Legacy);
            let input = LogEvent::from(bytes);
            let mut output = OutputBuffer::default();
            parser.transform(&mut output, input.into());

            assert!(output.is_empty(), "Expected no events: {:?}", output);
        }
    }
}
