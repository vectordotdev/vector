use chrono::{DateTime, Utc};
use derivative::Derivative;
use vector_lib::config::{log_schema, LegacyKey, LogNamespace};
use vector_lib::conversion;
use vector_lib::lookup::path;

use crate::sources::kubernetes_logs::transform_utils::get_message_path;
use crate::{
    event::{self, Event, Value},
    internal_events::{
        ParserConversionError, ParserMatchError, ParserMissingFieldError, DROP_EVENT,
    },
    sources::kubernetes_logs::Config,
    transforms::{FunctionTransform, OutputBuffer},
};

const STREAM_KEY: &str = "stream";
const TIMESTAMP_KEY: &str = "timestamp";

/// Parser for the CRI log format.
///
/// Expects logs to arrive in a CRI log format.
///
/// CRI log format ([documentation][cri_log_format]) is a simple
/// newline-separated text format. We rely on regular expressions to parse it.
///
/// Normalizes parsed data for consistency.
///
/// [cri_log_format]: https://github.com/kubernetes/community/blob/ee2abbf9dbfa4523b414f99a04ddc97bd38c74b2/contributors/design-proposals/node/kubelet-cri-logging.md
#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub(super) struct Cri {
    log_namespace: LogNamespace,
}

impl Cri {
    pub const fn new(log_namespace: LogNamespace) -> Self {
        Self { log_namespace }
    }
}

impl FunctionTransform for Cri {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        let message_path = get_message_path(self.log_namespace);

        // Get the log field with the message, if it exists, and coerce it to bytes.
        let log = event.as_mut_log();
        let value = log.remove(&message_path).map(|s| s.coerce_to_bytes());
        match value {
            None => {
                // The message field was missing, inexplicably. If we can't find the message field, there's nothing for
                // us to actually decode, so there's no event we could emit, and so we just emit the error and return.
                emit!(ParserMissingFieldError::<DROP_EVENT> {
                    field: &message_path.to_string()
                });
                return;
            }
            Some(s) => match parse_log_line(&s) {
                None => {
                    emit!(ParserMatchError { value: &s[..] });
                    return;
                }
                Some(parsed_log) => {
                    // For all fields except `timestamp`, simply treat them as `Value::Bytes`. For
                    // `timestamp`, however, we actually make sure we can convert it correctly and feed it
                    // in as `Value::Timestamp`.

                    // MESSAGE
                    // Insert either directly into `.` or `log_schema().message_key()`,
                    // overwriting the original "full" CRI log that included additional fields.
                    drop(log.insert(&message_path, Value::Bytes(s.slice_ref(parsed_log.message))));

                    // MULTILINE_TAG
                    // If the MULTILINE_TAG is 'P' (partial), insert our generic `_partial` key.
                    // This is safe to `unwrap()` as we've just ensured this value is a Value::Bytes
                    // during the above capturing and mapping.
                    if parsed_log.multiline_tag[0] == b'P' {
                        self.log_namespace.insert_source_metadata(
                            Config::NAME,
                            log,
                            Some(LegacyKey::Overwrite(path!(event::PARTIAL))),
                            path!(event::PARTIAL),
                            true,
                        );
                    }

                    // TIMESTAMP_TAG
                    let ds = String::from_utf8_lossy(parsed_log.timestamp);
                    match DateTime::parse_from_str(&ds, "%+") {
                        Ok(dt) =>
                        // Insert the TIMESTAMP_TAG parsed out of the CRI log, this is the timestamp of
                        // when the runtime processed this message.
                        {
                            self.log_namespace.insert_source_metadata(
                                Config::NAME,
                                log,
                                log_schema().timestamp_key().map(LegacyKey::Overwrite),
                                path!(TIMESTAMP_KEY),
                                Value::Timestamp(dt.with_timezone(&Utc)),
                            )
                        }
                        Err(e) => {
                            emit!(ParserConversionError {
                                name: TIMESTAMP_KEY,
                                error: conversion::Error::TimestampParse {
                                    s: ds.to_string(),
                                    source: e,
                                },
                            });
                        }
                    }

                    // STREAM_TAG
                    self.log_namespace.insert_source_metadata(
                        Config::NAME,
                        log,
                        Some(LegacyKey::Overwrite(path!(STREAM_KEY))),
                        path!(STREAM_KEY),
                        Value::Bytes(s.slice_ref(parsed_log.stream)),
                    );
                }
            },
        }

        output.push(event);
    }
}

struct ParsedLog<'a> {
    timestamp: &'a [u8],
    stream: &'a [u8],
    multiline_tag: &'a [u8],
    message: &'a [u8],
}

#[allow(clippy::trivially_copy_pass_by_ref)]
#[inline]
const fn is_delimiter(c: &u8) -> bool {
    *c == b' '
}

/// Parses a CRI log line.
///
/// Equivalent to regex: `(?-u)^(?P<timestamp>.*) (?P<stream>(stdout|stderr)) (?P<multiline_tag>(P|F)) (?P<message>.*)(?P<new_line_tag>\n?)$`
#[inline]
fn parse_log_line(line: &[u8]) -> Option<ParsedLog> {
    let rest = line;

    let after_timestamp_pos = rest.iter().position(is_delimiter)?;
    let (timestamp, rest) = rest.split_at(after_timestamp_pos + 1);
    let timestamp = timestamp.split_last()?.1; // Trim the delimiter

    let after_stream_pos = rest.iter().position(is_delimiter)?;
    let (stream, rest) = rest.split_at(after_stream_pos + 1);
    let stream = stream.split_last()?.1;
    if stream != b"stdout".as_ref() && stream != b"stderr".as_ref() {
        return None;
    }

    let after_multiline_tag_pos = rest.iter().position(is_delimiter)?;
    let (multiline_tag, rest) = rest.split_at(after_multiline_tag_pos + 1);
    let multiline_tag = multiline_tag.split_last()?.1;
    if multiline_tag != b"F".as_ref() && multiline_tag != b"P".as_ref() {
        return None;
    }

    let has_new_line_tag = !rest.is_empty() && *rest.last()? == b'\n';
    let message = if has_new_line_tag {
        // Remove the newline tag field, if it exists.
        // For additional details, see https://github.com/vectordotdev/vector/issues/8606.
        rest.split_last()?.1
    } else {
        rest
    };

    Some(ParsedLog {
        timestamp,
        stream,
        multiline_tag,
        message,
    })
}

#[cfg(test)]
pub mod tests {
    use bytes::Bytes;

    use super::{super::test_util, *};
    use crate::{event::LogEvent, test_util::trace_init};
    use vrl::value;

    fn make_long_string(base: &str, len: usize) -> String {
        base.chars().cycle().take(len).collect()
    }

    /// Shared test cases.
    pub fn valid_cases(log_namespace: LogNamespace) -> Vec<(Bytes, Vec<Event>)> {
        vec![
            (
                Bytes::from(
                    "2016-10-06T00:17:09.669794202Z stdout F The content of the log entry 1",
                ),
                vec![test_util::make_log_event(
                    value!("The content of the log entry 1"),
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    false,
                    log_namespace,
                )],
            ),
            (
                Bytes::from("2016-10-06T00:17:09.669794202Z stdout P First line of log entry 2"),
                vec![test_util::make_log_event(
                    value!("First line of log entry 2"),
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    true,
                    log_namespace,
                )],
            ),
            (
                Bytes::from(
                    "2016-10-06T00:17:09.669794202Z stdout P Second line of the log entry 2",
                ),
                vec![test_util::make_log_event(
                    value!("Second line of the log entry 2"),
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    true,
                    log_namespace,
                )],
            ),
            (
                Bytes::from("2016-10-06T00:17:10.113242941Z stderr F Last line of the log entry 2"),
                vec![test_util::make_log_event(
                    value!("Last line of the log entry 2"),
                    "2016-10-06T00:17:10.113242941Z",
                    "stderr",
                    false,
                    log_namespace,
                )],
            ),
            // A part of the partial message with a realistic length.
            (
                Bytes::from(
                    [
                        r#"2016-10-06T00:17:10.113242941Z stdout P "#,
                        make_long_string("very long message ", 16 * 1024).as_str(),
                    ]
                    .join(""),
                ),
                vec![test_util::make_log_event(
                    value!(make_long_string("very long message ", 16 * 1024)),
                    "2016-10-06T00:17:10.113242941Z",
                    "stdout",
                    true,
                    log_namespace,
                )],
            ),
            (
                // This is not valid UTF-8 string, ends with \n
                // 2021-08-05T17:35:26.640507539Z stdout P Hello World Привет Ми\xd1\n
                Bytes::from(vec![
                    50, 48, 50, 49, 45, 48, 56, 45, 48, 53, 84, 49, 55, 58, 51, 53, 58, 50, 54, 46,
                    54, 52, 48, 53, 48, 55, 53, 51, 57, 90, 32, 115, 116, 100, 111, 117, 116, 32,
                    80, 32, 72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 32, 208, 159, 209,
                    128, 208, 184, 208, 178, 208, 181, 209, 130, 32, 208, 156, 208, 184, 209, 10,
                ]),
                vec![test_util::make_log_event(
                    value!(Bytes::from(vec![
                        72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 32, 208, 159, 209, 128,
                        208, 184, 208, 178, 208, 181, 209, 130, 32, 208, 156, 208, 184, 209,
                    ])),
                    "2021-08-05T17:35:26.640507539Z",
                    "stdout",
                    true,
                    log_namespace,
                )],
            ),
        ]
    }

    #[test]
    fn test_parsing_valid_vector_namespace() {
        trace_init();
        test_util::test_parser(
            || Cri::new(LogNamespace::Vector),
            |bytes| Event::Log(LogEvent::from(value!(bytes))),
            valid_cases(LogNamespace::Vector),
        );
    }

    #[test]
    fn test_parsing_valid_legacy_namespace() {
        trace_init();
        test_util::test_parser(
            || Cri::new(LogNamespace::Legacy),
            |bytes| Event::Log(LogEvent::from(bytes)),
            valid_cases(LogNamespace::Legacy),
        );
    }
}
