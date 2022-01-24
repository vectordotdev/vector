use derivative::Derivative;
use shared::TimeZone;
use snafu::{OptionExt, Snafu};

use crate::{
    event::{self, Event, LogEvent, Value},
    transforms::{
        regex_parser::{RegexParser, RegexParserConfig},
        FunctionTransform, OutputBuffer,
    },
};

pub const MULTILINE_TAG: &str = "multiline_tag";
pub const NEW_LINE_TAG: &str = "new_line_tag";

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
pub struct Cri {
    #[derivative(Debug = "ignore")]
    regex_parser: Box<dyn FunctionTransform>,
}

impl Cri {
    /// Create a new [`Cri`] parser.
    pub fn new(timezone: TimeZone) -> Self {
        let regex_parser = {
            let mut rp_config = RegexParserConfig::default();

            let pattern = r"(?-u)^(?P<timestamp>.*) (?P<stream>(stdout|stderr)) (?P<multiline_tag>(P|F)) (?P<message>.*)(?P<new_line_tag>\n?)$";
            rp_config.patterns = vec![pattern.to_owned()];

            rp_config.types.insert(
                crate::config::log_schema().timestamp_key().to_string(),
                "timestamp|%+".to_owned(),
            );

            let parser = RegexParser::build(&rp_config, timezone)
                .expect("regexp patterns are static, should never fail");
            parser.into_function()
        };

        Self { regex_parser }
    }
}

impl FunctionTransform for Cri {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        let mut buf = OutputBuffer::with_capacity(1);
        self.regex_parser.transform(&mut buf, event);
        if let Some(mut event) = buf.into_events().next() {
            if normalize_event(event.as_mut_log()).ok().is_some() {
                output.push(event);
            }
        }
    }
}

fn normalize_event(log: &mut LogEvent) -> Result<(), NormalizationError> {
    // Remove possible new_line tag
    // for additional details, see https://github.com/timberio/vector/issues/8606
    let _ = log.remove(NEW_LINE_TAG);
    // Detect if this is a partial event.
    let multiline_tag = log
        .remove(MULTILINE_TAG)
        .context(MultilineTagFieldMissingSnafu)?;
    let multiline_tag = match multiline_tag {
        Value::Bytes(val) => val,
        _ => return Err(NormalizationError::MultilineTagValueUnexpectedType),
    };

    let is_partial = multiline_tag[0] == b'P';

    // For partial messages add a partial event indicator.
    if is_partial {
        log.insert(event::PARTIAL, true);
    }

    Ok(())
}

#[derive(Debug, Snafu)]
enum NormalizationError {
    MultilineTagFieldMissing,
    MultilineTagValueUnexpectedType,
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
    pub fn cases() -> Vec<(String, Vec<LogEvent>)> {
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

    pub fn byte_cases() -> Vec<(Bytes, Vec<LogEvent>)> {
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
            || Transform::function(Cri::new(TimeZone::Local)),
            Event::from,
            cases(),
        );
    }

    #[test]
    fn test_parsing_bytes() {
        trace_init();
        test_util::test_parser(
            || Transform::function(Cri::new(TimeZone::Local)),
            Event::from,
            byte_cases(),
        );
    }
}
