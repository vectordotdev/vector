#![cfg(test)]

use crate::{
    event::{Event, LogEvent, LookupBuf},
    transforms::Transform,
};
use chrono::{DateTime, Utc};

/// Build a log event for test purposes.
///
/// The implementation is shared, and therefore consistent across all
/// the parsers.
pub fn make_log_event(message: &str, timestamp: &str, stream: &str, is_partial: bool) -> LogEvent {
    let mut log = LogEvent::default();

    log.insert(LookupBuf::from("message"), message);

    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .expect("invalid test case")
        .with_timezone(&Utc);
    log.insert(LookupBuf::from("timestamp"), timestamp);

    log.insert(LookupBuf::from("stream"), stream);

    if is_partial {
        log.insert(LookupBuf::from("_partial"), true);
    }
    log
}

/// Shared logic for testing parsers.
///
/// Takes a parser builder and a list of test cases.
pub fn test_parser<B>(builder: B, cases: Vec<(String, LogEvent)>)
where
    B: Fn() -> Transform,
{
    for (message, expected) in cases {
        let input = Event::from(message);
        let mut parser = (builder)();
        let parser = parser.as_function();

        let output = parser
            .transform_one(input)
            .expect("parser failed to parse the event");
        assert_eq!(Event::Log(expected), output, "expected left, actual right");
    }
}
