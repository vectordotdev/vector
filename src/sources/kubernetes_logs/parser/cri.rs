use bytes::Bytes;
use chrono::{DateTime, Utc};
use derivative::Derivative;
use lookup::path;
use regex::bytes::{CaptureLocations, Regex};
use vector_common::conversion;
use vector_config::NamedComponent;
use vector_core::config::{log_schema, LegacyKey, LogNamespace};

use crate::{
    event::{self, Event, Value},
    internal_events::{
        ParserConversionError, ParserMatchError, ParserMissingFieldError, DROP_EVENT,
    },
    sources::kubernetes_logs::Config,
    transforms::{FunctionTransform, OutputBuffer},
};

const CRI_REGEX_PATTERN: &str = r"(?-u)^(?P<timestamp>.*) (?P<stream>(stdout|stderr)) (?P<multiline_tag>(P|F)) (?P<message>.*)\n?$";
const MESSAGE_KEY: &str = "message";
const MULTILINE_KEY: &str = "multiline_tag";
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
    #[derivative(Debug = "ignore")]
    pattern: Regex,
    capture_locations: CaptureLocations,
    capture_names: Vec<(usize, String)>,
    log_namespace: LogNamespace,
}

impl Cri {
    pub fn new(log_namespace: LogNamespace) -> Self {
        let pattern =
            Regex::new(CRI_REGEX_PATTERN).expect("CRI log regex pattern should never fail");

        let capture_names = pattern
            .capture_names()
            .enumerate()
            .filter_map(|(i, s)| s.map(|s| (i, s.to_string())))
            .collect::<Vec<_>>();
        let capture_locations = pattern.capture_locations();

        Self {
            pattern,
            capture_locations,
            capture_names,
            log_namespace,
        }
    }
}

impl FunctionTransform for Cri {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        let message_field = match self.log_namespace {
            LogNamespace::Vector => ".",
            LogNamespace::Legacy => log_schema().message_key(),
        };

        // Get the log field with the message, if it exists, and coerce it to bytes.
        let log = event.as_mut_log();
        let value = log.remove(message_field).map(|s| s.coerce_to_bytes());
        match value {
            None => {
                // The message field was missing, inexplicably. If we can't find the message field, there's nothing for
                // us to actually decode, so there's no event we could emit, and so we just emit the error and return.
                emit!(ParserMissingFieldError::<DROP_EVENT> {
                    field: message_field
                });
                return;
            }
            Some(s) => match self.pattern.captures_read(&mut self.capture_locations, &s) {
                None => {
                    emit!(ParserMatchError { value: &s[..] });
                    return;
                }
                Some(_) => {
                    let locations = &self.capture_locations;
                    let captures = self.capture_names.iter().filter_map(|(idx, name)| {
                        locations.get(*idx).and_then(|(start, end)| {
                            let raw = &s[start..end];

                            // For all fields except `timestamp`, simply treat them as `Value::Bytes`. For
                            // `timestamp`, however, we actually make sure we can convert it correctly and feed it
                            // in as `Value::Timestamp`.
                            let value = if name == TIMESTAMP_KEY {
                                let ds = String::from_utf8_lossy(raw);
                                match DateTime::parse_from_str(&ds, "%+") {
                                    Ok(dt) => Some(Value::Timestamp(dt.with_timezone(&Utc))),
                                    Err(e) => {
                                        emit!(ParserConversionError {
                                            name,
                                            error: conversion::Error::TimestampParse {
                                                s: ds.to_string(),
                                                source: e,
                                            },
                                        });
                                        None
                                    }
                                }
                            } else {
                                Some(Value::Bytes(Bytes::copy_from_slice(raw)))
                            };

                            value.map(|v| (name, v))
                        })
                    });

                    for (name, value) in captures {
                        match name.as_str() {
                            MESSAGE_KEY => {
                                // Insert either directly into `.` or `log_schema().message_key()`,
                                // overwriting the original "full" CRI log that included additional fields.
                                drop(log.insert(message_field, value));
                            }
                            MULTILINE_KEY => {
                                // If the MULTILINE_TAG is 'P' (partial), insert our generic `_partial` key.
                                // This is safe to `unwrap()` as we've just ensured this value is a Value::Bytes
                                // during the above capturing and mapping.
                                if value.as_bytes().unwrap()[0] == b'P' {
                                    self.log_namespace.insert_source_metadata(
                                        Config::NAME,
                                        log,
                                        Some(LegacyKey::Overwrite(path!(event::PARTIAL))),
                                        path!(event::PARTIAL),
                                        true,
                                    );
                                }
                            }
                            TIMESTAMP_KEY => {
                                // Insert the TIMESTAMP_TAG parsed out of the CRI log, this is the timestamp of
                                // when the runtime processed this message.
                                self.log_namespace.insert_source_metadata(
                                    Config::NAME,
                                    log,
                                    Some(LegacyKey::Overwrite(path!(log_schema().timestamp_key()))),
                                    path!(TIMESTAMP_KEY),
                                    value,
                                );
                            }
                            STREAM_KEY => {
                                self.log_namespace.insert_source_metadata(
                                    Config::NAME,
                                    log,
                                    Some(LegacyKey::Overwrite(path!(STREAM_KEY))),
                                    path!(STREAM_KEY),
                                    value,
                                );
                            }
                            _ => {
                                unreachable!("all CRI captures groups should be matched");
                            }
                        }
                    }
                }
            },
        }

        output.push(event);
    }
}

#[cfg(test)]
pub mod tests {
    use bytes::Bytes;

    use super::{super::test_util, *};
    use crate::{event::LogEvent, test_util::trace_init, transforms::Transform};

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
                    vrl::value!("The content of the log entry 1"),
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    false,
                    log_namespace,
                )],
            ),
            (
                Bytes::from("2016-10-06T00:17:09.669794202Z stdout P First line of log entry 2"),
                vec![test_util::make_log_event(
                    vrl::value!("First line of log entry 2"),
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
                    vrl::value!("Second line of the log entry 2"),
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    true,
                    log_namespace,
                )],
            ),
            (
                Bytes::from("2016-10-06T00:17:10.113242941Z stderr F Last line of the log entry 2"),
                vec![test_util::make_log_event(
                    vrl::value!("Last line of the log entry 2"),
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
                    vrl::value!(make_long_string("very long message ", 16 * 1024)),
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
                    vrl::value!(Bytes::from(vec![
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
            || Transform::function(Cri::new(LogNamespace::Vector)),
            |bytes| Event::Log(LogEvent::from(vrl::value!(bytes))),
            valid_cases(LogNamespace::Vector),
        );
    }

    #[test]
    fn test_parsing_valid_legacy_namespace() {
        trace_init();
        test_util::test_parser(
            || Transform::function(Cri::new(LogNamespace::Legacy)),
            |bytes| Event::Log(LogEvent::from(bytes)),
            valid_cases(LogNamespace::Legacy),
        );
    }
}
