#![cfg(test)]
use similar_asserts::assert_eq;

use chrono::{DateTime, Utc};
use vector_lib::lookup::{event_path, metadata_path};
use vector_lib::{config::LogNamespace, event};
use vrl::value;
use vrl::value::Value;

use crate::{
    event::{Event, LogEvent},
    sources::kubernetes_logs::Config,
    transforms::{FunctionTransform, OutputBuffer},
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
    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .expect("invalid timestamp in test case")
        .with_timezone(&Utc);

    let log = match log_namespace {
        LogNamespace::Vector => {
            let mut log = LogEvent::from(value!(message));
            log.insert(metadata_path!(Config::NAME, "timestamp"), timestamp);
            log.insert(metadata_path!(Config::NAME, "stream"), stream);
            if is_partial {
                log.insert(metadata_path!(Config::NAME, event::PARTIAL), true);
            }

            log
        }
        LogNamespace::Legacy => {
            let mut log = LogEvent::default();

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
pub fn test_parser<B, L, S, F>(builder: B, loader: L, cases: Vec<(S, Vec<Event>)>)
where
    B: Fn() -> F,
    F: FunctionTransform,
    L: Fn(S) -> Event,
{
    for (message, expected) in cases {
        let input = loader(message);
        let mut parser = (builder)();
        let mut output = OutputBuffer::default();
        parser.transform(&mut output, input);

        let actual = output.into_events().collect::<Vec<_>>();

        assert_eq!(expected, actual, "expected left, actual right");
    }
}
