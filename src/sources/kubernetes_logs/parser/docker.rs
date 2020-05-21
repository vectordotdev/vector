use crate::{
    event::{self, Event, LogEvent, Value},
    transforms::{
        json_parser::{JsonParser, JsonParserConfig},
        Transform,
    },
};
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use snafu::{OptionExt, ResultExt, Snafu};
use string_cache::DefaultAtom as Atom;

pub fn build() -> Box<dyn Transform> {
    Box::new(Docker::new())
}

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
pub struct Docker {
    json_parser: JsonParser,
}

impl Docker {
    /// Create a new [`Docker`] parser.
    pub fn new() -> Self {
        let json_parser = {
            let mut config = JsonParserConfig::default();
            config.drop_field = true;

            // Drop so that it's possible to detect if message is in json format.
            config.drop_invalid = true;

            config.into()
        };

        Self { json_parser }
    }
}

impl Transform for Docker {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut event = self.json_parser.transform(event)?;
        normalize_event(event.as_mut_log()).ok()?;
        Some(event)
    }
}

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
    let is_partial = if message.last().map(|&b| b as char == '\n').unwrap_or(false) {
        message.truncate(message.len() - 1);
        false
    } else {
        true
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
mod tests {
    use super::super::test_util;
    use super::*;

    #[test]
    fn test_parsing() {
        let cases = vec![
            (
                r#"{"log": "The actual log line\n", "stream": "stderr", "time": "2016-10-05T00:00:30.082640485Z"}"#,
                test_util::make_log_event(
                    "The actual log line",
                    "2016-10-05T00:00:30.082640485Z",
                    "stderr",
                    false,
                ),
            ),
            (
                r#"{"log": "The partial log line", "stream": "stdout", "time": "2016-10-05T00:00:30.082640485Z"}"#,
                test_util::make_log_event(
                    "The partial log line",
                    "2016-10-05T00:00:30.082640485Z",
                    "stdout",
                    true,
                ),
            ),
        ];

        test_util::test_parser(build, cases);
    }
}
