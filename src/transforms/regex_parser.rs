use super::Transform;
use crate::event::{self, Event};
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct RegexParserConfig {
    pub regex: String,
    pub field: Option<Atom>,
    pub drop_field: bool,
}

#[typetag::serde(name = "regex_parser")]
impl crate::topology::config::TransformConfig for RegexParserConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        let field = if let Some(field) = &self.field {
            field
        } else {
            &event::MESSAGE
        };

        Regex::new(&self.regex)
            .map_err(|err| err.to_string())
            .map::<Box<dyn Transform>, _>(|r| {
                Box::new(RegexParser::new(r, field.clone(), self.drop_field))
            })
    }
}

pub struct RegexParser {
    regex: Regex,
    field: Atom,
    drop_field: bool,
}

impl RegexParser {
    pub fn new(regex: Regex, field: Atom, drop_field: bool) -> Self {
        Self {
            regex,
            field,
            drop_field,
        }
    }
}

impl Transform for RegexParser {
    fn transform(&self, mut event: Event) -> Option<Event> {
        let value = event.as_log().get(&self.field).map(|s| s.as_bytes());

        if let Some(value) = &value {
            if let Some(captures) = self.regex.captures(&value) {
                let mut do_drop = self.drop_field;
                for name in self.regex.capture_names().filter_map(|c| c) {
                    if name == self.field.as_ref() {
                        do_drop = false;
                    }
                    if let Some(capture) = captures.name(name) {
                        event
                            .as_mut_log()
                            .insert_explicit(name.into(), capture.as_bytes().into());
                    }
                }
                if do_drop {
                    event.as_mut_log().remove(&self.field);
                }
            } else {
                debug!(message = "No fields captured from regex");
            }
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.field.as_ref(),
            );
        };

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::RegexParserConfig;
    use crate::event::LogEvent;
    use crate::{topology::config::TransformConfig, Event};

    fn do_transform(event: &str, regex: &str, field: Option<&str>, drop_field: bool) -> LogEvent {
        let event = Event::from(event);
        let parser = RegexParserConfig {
            regex: regex.into(),
            field: field.map(|field| field.into()),
            drop_field,
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap();

        event.into_log()
    }

    #[test]
    fn regex_parser_adds_parsed_field_to_event() {
        let log = do_transform(
            "status=1234 time=5678",
            r"status=(?P<status>\d+) time=(?P<time>\d+)",
            None,
            false,
        );

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn regex_parser_doesnt_do_anything_if_no_match() {
        let log = do_transform("asdf1234", r"status=(?P<status>\d+)", None, false);

        assert_eq!(log.get(&"status".into()), None);
        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn regex_parser_does_drop_parsed_field() {
        let log = do_transform(
            "status=1234 time=5678",
            r"status=(?P<status>\d+) time=(?P<time>\d+)",
            Some("message"),
            true,
        );

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_none());
    }

    #[test]
    fn regex_parser_does_not_drop_same_name_parsed_field() {
        let log = do_transform(
            "status=1234 message=yes",
            r"status=(?P<status>\d+) message=(?P<message>\S+)",
            Some("message"),
            true,
        );

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"message".into()], "yes".into());
    }

    #[test]
    fn regex_parser_does_not_drop_if_no_match() {
        let log = do_transform(
            "asdf1234",
            r"status=(?P<message>\S+)",
            Some("message"),
            true,
        );

        assert!(log.get(&"message".into()).is_some());
    }
}
