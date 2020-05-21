use crate::{
    event::{self, Event, LogEvent, Value},
    transforms::{
        regex_parser::{RegexParser, RegexParserConfig},
        Transform,
    },
};
use lazy_static::lazy_static;
use snafu::{OptionExt, Snafu};
use string_cache::DefaultAtom as Atom;

pub fn build() -> Box<dyn Transform> {
    Box::new(Cri::new())
}

lazy_static! {
    pub static ref MULTILINE_TAG: Atom = Atom::from("multiline_tag");
}

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
pub struct Cri {
    // TODO: patch `RegexParser` to expose the concerete type on build.
    regex_parser: Box<dyn Transform>,
}

impl Cri {
    /// Create a new [`Cri`] parser.
    pub fn new() -> Self {
        let regex_parser = {
            let mut rp_config = RegexParserConfig::default();

            let pattern = r"^(?P<timestamp>.*) (?P<stream>(stdout|stderr)) (?P<multiline_tag>(P|F)) (?P<message>.*)$";
            rp_config.patterns = vec![pattern.to_owned()];

            rp_config.types.insert(
                event::log_schema().timestamp_key().clone(),
                "timestamp|%+".to_owned(),
            );

            RegexParser::build(&rp_config).expect("regexp patterns are static, should never fail")
        };

        Self { regex_parser }
    }
}

impl Transform for Cri {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut event = self.regex_parser.transform(event)?;
        normalize_event(event.as_mut_log()).ok()?;
        Some(event)
    }
}

fn normalize_event(log: &mut LogEvent) -> Result<(), NormalizationError> {
    // Detect if this is a partial event.
    let multiline_tag = log
        .remove(&MULTILINE_TAG)
        .context(MultilineTagFieldMissing)?;
    let multiline_tag = match multiline_tag {
        Value::Bytes(val) => val,
        _ => return Err(NormalizationError::MultilineTagValueUnexpectedType),
    };

    let is_partial = multiline_tag[0] == b'P';

    // For partial messages add a partial event indicator.
    if is_partial {
        log.insert(event::PARTIAL_STR, true);
    }

    Ok(())
}

#[derive(Debug, Snafu)]
enum NormalizationError {
    MultilineTagFieldMissing,
    MultilineTagValueUnexpectedType,
}

#[cfg(test)]
mod tests {
    use super::super::test_util;
    use super::*;

    #[test]
    fn test_parsing() {
        let cases = vec![
            (
                "2016-10-06T00:17:09.669794202Z stdout F The content of the log entry 1",
                test_util::make_log_event(
                    "The content of the log entry 1",
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    false,
                ),
            ),
            (
                "2016-10-06T00:17:09.669794202Z stdout P First line of log entry 2",
                test_util::make_log_event(
                    "First line of log entry 2",
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    true,
                ),
            ),
            (
                "2016-10-06T00:17:09.669794202Z stdout P Second line of the log entry 2",
                test_util::make_log_event(
                    "Second line of the log entry 2",
                    "2016-10-06T00:17:09.669794202Z",
                    "stdout",
                    true,
                ),
            ),
            (
                "2016-10-06T00:17:10.113242941Z stderr F Last line of the log entry 2",
                test_util::make_log_event(
                    "Last line of the log entry 2",
                    "2016-10-06T00:17:10.113242941Z",
                    "stderr",
                    false,
                ),
            ),
        ];

        test_util::test_parser(build, cases);
    }
}
