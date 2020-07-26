use crate::{
    event::{self, Event, LogEvent, Value},
    transforms::Transform,
};
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use serde_json::Value as JsonValue;
use snafu::{OptionExt, ResultExt, Snafu};
use string_cache::DefaultAtom as Atom;

lazy_static! {
    pub static ref TIME: Atom = Atom::from("time");
    pub static ref LOG: Atom = Atom::from("log");
}

/// Parser for the docker log format.
///
/// Expects logs to arrive in a JSONLines format with the fields names and
/// contents specific to the implementation of the Docker `json` log driver.
///
/// Normalizes parsed data for consistency.
#[derive(Debug)]
pub struct Docker;

impl Transform for Docker {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();
        parse_json(log)?;
        normalize_event(log).ok()?;
        Some(event)
    }
}

/// Parses `message` as json object and removes it.
fn parse_json(log: &mut LogEvent) -> Option<()> {
    let to_parse = log.remove(&event::log_schema().message_key())?.as_bytes();

    match serde_json::from_slice(to_parse.as_ref()) {
        Ok(JsonValue::Object(object)) => {
            for (key, value) in object {
                log.insert_flat(key, value);
            }
            Some(())
        }
        Ok(_) | Err(_) => None,
    }
}

const DOCKER_MESSAGE_SPLIT_THRESHOLD: usize = 16 * 1024; // 16 Kib

fn normalize_event(log: &mut LogEvent) -> Result<(), NormalizationError> {
    // Parse and rename timestamp.
    let time = log.remove(&TIME).context(TimeFieldMissing)?;
    let time = match time {
        Value::Bytes(val) => val,
        _ => return Err(NormalizationError::TimeValueUnexpectedType),
    };
    let time = DateTime::parse_from_rfc3339(String::from_utf8_lossy(time.as_ref()).as_ref())
        .context(TimeParsing)?;
    log.insert(
        event::log_schema().timestamp_key(),
        time.with_timezone(&Utc),
    );

    // Parse message, remove trailing newline and detect if it's partial.
    let message = log.remove(&LOG).context(LogFieldMissing)?;
    let mut message = match message {
        Value::Bytes(val) => val,
        _ => return Err(NormalizationError::LogValueUnexpectedType),
    };
    // Here we apply out heuristics to detect if messge is partial.
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
    log.insert(event::log_schema().message_key(), message);

    // For partial messages add a partial event indicator.
    if is_partial {
        log.insert(event::PARTIAL_STR, true);
    }

    Ok(())
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
    use super::super::test_util;
    use super::Docker;
    use crate::event::LogEvent;

    fn make_long_string(base: &str, len: usize) -> String {
        base.chars().cycle().take(len).collect()
    }

    /// Shared test cases.
    pub fn cases() -> Vec<(String, LogEvent)> {
        vec![
            (
                r#"{"log": "The actual log line\n", "stream": "stderr", "time": "2016-10-05T00:00:30.082640485Z"}"#.into(),
                test_util::make_log_event(
                    "The actual log line",
                    "2016-10-05T00:00:30.082640485Z",
                    "stderr",
                    false,
                ),
            ),
            (
                r#"{"log": "A line without newline chan at the end", "stream": "stdout", "time": "2016-10-05T00:00:30.082640485Z"}"#.into(),
                test_util::make_log_event(
                    "A line without newline chan at the end",
                    "2016-10-05T00:00:30.082640485Z",
                    "stdout",
                    false,
                ),
            ),
            // Partial message due to message length.
            (
                [
                    r#"{"log": ""#,
                    make_long_string("partial ", 16 * 1024).as_str(),
                    r#"", "stream": "stdout", "time": "2016-10-05T00:00:30.082640485Z"}"#,
                ]
                .join(""),
                test_util::make_log_event(
                    make_long_string("partial ",16 * 1024).as_str(),
                    "2016-10-05T00:00:30.082640485Z",
                    "stdout",
                    true,
                ),
            ),
            // Non-partial message, because message length matches but
            // the message also ends with newline.
            (
                [
                    r#"{"log": ""#,
                    make_long_string("non-partial ", 16 * 1024 - 1).as_str(),
                    r"\n",
                    r#"", "stream": "stdout", "time": "2016-10-05T00:00:30.082640485Z"}"#,
                ]
                .join(""),
                test_util::make_log_event(
                    make_long_string("non-partial ", 16 * 1024 - 1).as_str(),
                    "2016-10-05T00:00:30.082640485Z",
                    "stdout",
                    false,
                ),
            ),
        ]
    }

    #[test]
    fn test_parsing() {
        test_util::test_parser(|| Docker, cases());
    }
}
