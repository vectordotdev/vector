#![cfg(test)]
// TODO: Insert with log_namespace
use similar_asserts::assert_eq;

use chrono::{DateTime, Utc};
use lookup::event_path;
use value::Value;
use vector_core::{config::LogNamespace, event};

use crate::{
    event::{Event, LogEvent},
    transforms::{OutputBuffer, Transform},
};

/// Build a log event for test purposes.
///
/// The implementation is shared, and therefore consistent across all
/// the parsers.
pub fn make_log_event(
    message: Value,
    timestamp: &str,
    stream: &str,
    is_partial: bool,
    log_namespace: LogNamespace,
) -> Event {
    let log = match log_namespace {
        LogNamespace::Vector => LogEvent::from(vrl::value!("hello world")),
        LogNamespace::Legacy => {
            let mut log = LogEvent::default();
            let timestamp = DateTime::parse_from_rfc3339(timestamp)
                .expect("invalid test case")
                .with_timezone(&Utc);

            log.insert(event_path!("message"), message);
            log.insert(event_path!("timestamp"), timestamp);
            log.insert(event_path!("stream"), stream);
            if is_partial {
                log.insert(event_path!(event::PARTIAL), true);
            }

            log
        }
    };

    Event::Log(log)
}

/// Shared logic for testing parsers.
///
/// Takes a parser builder and a list of test cases.
pub fn test_parser<B, L, S>(builder: B, loader: L, cases: Vec<(S, Vec<Event>)>)
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

        let actual = output.into_events().collect::<Vec<_>>();

        assert_eq!(expected, actual, "expected left, actual right");
    }
}
