#![cfg(test)]

use bytes::Bytes;
use chrono::{DateTime, Utc};

use crate::{
    event::{Event, LogEvent},
    transforms::{OutputBuffer, Transform},
};

/// Build a log event for test purposes.
///
/// The implementation is shared, and therefore consistent across all
/// the parsers.
pub fn make_log_event(message: &str, timestamp: &str, stream: &str, is_partial: bool) -> LogEvent {
    let mut log = LogEvent::default();

    log.insert("message", message);

    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .expect("invalid test case")
        .with_timezone(&Utc);
    log.insert("timestamp", timestamp);

    log.insert("stream", stream);

    if is_partial {
        log.insert("_partial", true);
    }
    log
}

/// Build a log event for test purposes.
/// Message can be a not valid UTF-8 string
///
/// The implementation is shared, and therefore consistent across all
/// the parsers.
pub fn make_log_event_with_byte_message(
    message: Bytes,
    timestamp: &str,
    stream: &str,
    is_partial: bool,
) -> LogEvent {
    let mut log = LogEvent::default();

    log.insert("message", message);

    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .expect("invalid test case")
        .with_timezone(&Utc);
    log.insert("timestamp", timestamp);

    log.insert("stream", stream);

    if is_partial {
        log.insert("_partial", true);
    }
    log
}

/// Shared logic for testing parsers.
///
/// Takes a parser builder and a list of test cases.
pub fn test_parser<B, L, S>(builder: B, loader: L, cases: Vec<(S, Vec<LogEvent>)>)
where
    B: Fn() -> Transform,
    L: Fn(S) -> Event,
{
    for (message, expected) in cases {
        let input = loader(message);
        let mut parser = (builder)();
        let parser = parser.as_function();

        let mut output = OutputBuffer::default();
        parser.transform(&mut output, input);

        let expected = expected.into_iter().map(Event::Log).collect::<Vec<_>>();

        shared::assert_event_data_eq!(output, expected, "expected left, actual right");
    }
}
