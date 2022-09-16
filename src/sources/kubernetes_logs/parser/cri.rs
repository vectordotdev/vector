use bytes::Bytes;
use chrono::{DateTime, Utc};
use derivative::Derivative;
use regex::bytes::{CaptureLocations, Regex};
use vector_common::conversion;

use crate::{
    event::{self, Event, Value},
    internal_events::{
        ParserConversionError, ParserMatchError, ParserMissingFieldError, DROP_EVENT,
    },
    transforms::{FunctionTransform, OutputBuffer},
};

pub const MULTILINE_TAG: &str = "multiline_tag";
pub const NEW_LINE_TAG: &str = "new_line_tag";
const TIMESTAMP_TAG: &str = "timestamp";
const CRI_REGEX_PATTERN: &str = r"(?-u)^(?P<timestamp>.*) (?P<stream>(stdout|stderr)) (?P<multiline_tag>(P|F)) (?P<message>.*)(?P<new_line_tag>\n?)$";

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
    field: &'static str,
}

impl Default for Cri {
    fn default() -> Self {
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
            field: crate::config::log_schema().message_key(),
        }
    }
}

impl FunctionTransform for Cri {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        // Get the log field with the message, if it exists, and coerce it to bytes.
        let log = event.as_mut_log();
        let value = log.get(self.field).map(|s| s.coerce_to_bytes());
        match value {
            None => {
                // The message field was missing, inexplicably. If we can't find the message field, there's nothing for
                // us to actually decode, so there's no event we could emit, and so we just emit the error and return.
                emit!(ParserMissingFieldError::<DROP_EVENT> { field: self.field });
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
                            let value = if name == TIMESTAMP_TAG {
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

                    let mut drop_original = true;
                    for (name, value) in captures {
                        // If we're already overriding the original field, don't remove it after.
                        if name == self.field {
                            drop_original = false;
                        }

                        drop(log.insert(name.as_str(), value));
                    }

                    // If we didn't overwrite the original field, remove it now.
                    if drop_original {
                        drop(log.remove(self.field));
                    }
                }
            },
        }

        // Remove the newline tag field, if it exists.
        //
        // For additional details, see https://github.com/vectordotdev/vector/issues/8606.
        let _ = log.remove(NEW_LINE_TAG);

        // Detect if this is a partial event by examining the multiline tag field, and if it is, convert it to the more
        // generic `_partial` field that partial event merger will be looking for.
        match log.remove(MULTILINE_TAG) {
            Some(Value::Bytes(val)) => {
                let is_partial = val[0] == b'P';
                if is_partial {
                    log.insert(event::PARTIAL, true);
                }
            }
            _ => {
                // The multiline tag always needs to exist in the message, and it needs to be a string, so if we didn't
                // find it, or it's not a string, this is an invalid event overall so we don't emit the event.

                // TODO: Should we actually emit an internal event/error here? It would definitely be weird if a
                // mandated field in the log format wasn't present/the right type.
                return;
            }
        };

        // Since we successfully parsed the message, send it onward.
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
    pub fn cases() -> Vec<(String, Vec<Event>)> {
        vec![
            (
                "2016-10-06T00:17:09.669794202Z stdout F The content of the log entry 1".into(),
                vec![test_util::make_log_event(
                    "The content of the log entry 1",
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    false,
                )],
            ),
            (
                "2016-10-06T00:17:09.669794202Z stdout P First line of log entry 2".into(),
                vec![test_util::make_log_event(
                    "First line of log entry 2",
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    true,
                )],
            ),
            (
                "2016-10-06T00:17:09.669794202Z stdout P Second line of the log entry 2".into(),
                vec![test_util::make_log_event(
                    "Second line of the log entry 2",
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    true,
                )],
            ),
            (
                "2016-10-06T00:17:10.113242941Z stderr F Last line of the log entry 2".into(),
                vec![test_util::make_log_event(
                    "Last line of the log entry 2",
                    "2016-10-06T00:17:10.113242941Z",
                    "stderr",
                    false,
                )],
            ),
            // A part of the partial message with a realistic length.
            (
                [
                    r#"2016-10-06T00:17:10.113242941Z stdout P "#,
                    make_long_string("very long message ", 16 * 1024).as_str(),
                ]
                .join(""),
                vec![test_util::make_log_event(
                    make_long_string("very long message ", 16 * 1024).as_str(),
                    "2016-10-06T00:17:10.113242941Z",
                    "stdout",
                    true,
                )],
            ),
        ]
    }

    pub fn byte_cases() -> Vec<(Bytes, Vec<Event>)> {
        vec![(
            // This is not valid UTF-8 string, ends with \n
            // 2021-08-05T17:35:26.640507539Z stdout P Hello World Привет Ми\xd1\n
            Bytes::from(vec![
                50, 48, 50, 49, 45, 48, 56, 45, 48, 53, 84, 49, 55, 58, 51, 53, 58, 50, 54, 46, 54,
                52, 48, 53, 48, 55, 53, 51, 57, 90, 32, 115, 116, 100, 111, 117, 116, 32, 80, 32,
                72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 32, 208, 159, 209, 128, 208,
                184, 208, 178, 208, 181, 209, 130, 32, 208, 156, 208, 184, 209, 10,
            ]),
            vec![test_util::make_log_event_with_byte_message(
                Bytes::from(vec![
                    72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 32, 208, 159, 209, 128,
                    208, 184, 208, 178, 208, 181, 209, 130, 32, 208, 156, 208, 184, 209,
                ]),
                "2021-08-05T17:35:26.640507539Z",
                "stdout",
                true,
            )],
        )]
    }

    #[test]
    fn test_parsing() {
        trace_init();
        test_util::test_parser(
            || Transform::function(Cri::default()),
            |s| Event::Log(LogEvent::from(s)),
            cases(),
        );
    }

    #[test]
    fn test_parsing_bytes() {
        trace_init();
        test_util::test_parser(
            || Transform::function(Cri::default()),
            |bytes| LogEvent::from(bytes).into(),
            byte_cases(),
        );
    }
}
