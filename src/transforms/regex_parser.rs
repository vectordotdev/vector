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
    pub drop_failed: bool,
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
                Box::new(RegexParser::new(
                    r,
                    field.clone(),
                    self.drop_field,
                    self.drop_failed,
                ))
            })
    }
}

pub struct RegexParser {
    regex: Regex,
    field: Atom,
    capture_names: Vec<String>,
    drop_field: bool,
    drop_failed: bool,
}

impl RegexParser {
    pub fn new(regex: Regex, field: Atom, mut drop_field: bool, drop_failed: bool) -> Self {
        let capture_names: Vec<String> = regex
            .capture_names()
            .filter_map(|c| c.map(|c| c.into()))
            .collect();
        for name in &capture_names {
            if name == field.as_ref() {
                drop_field = false;
            }
        }
        Self {
            regex,
            field,
            capture_names,
            drop_field,
            drop_failed,
        }
    }
}

impl Transform for RegexParser {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let value = event.as_log().get(&self.field).map(|s| s.as_bytes());

        if let Some(value) = &value {
            if let Some(captures) = self.regex.captures(&value) {
                for name in &self.capture_names {
                    if let Some(capture) = captures.name(&name) {
                        event
                            .as_mut_log()
                            .insert_explicit(name.as_str().into(), capture.as_bytes().into());
                    }
                }
                if self.drop_field {
                    event.as_mut_log().remove(&self.field);
                }
                return Some(event);
            } else {
                debug!(message = "No fields captured from regex");
            }
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.field.as_ref(),
            );
        }

        if self.drop_failed {
            None
        } else {
            Some(event)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RegexParserConfig;
    use crate::event::LogEvent;
    use crate::{topology::config::TransformConfig, Event};

    fn do_transform(
        event: &str,
        regex: &str,
        field: Option<&str>,
        drop_field: bool,
        drop_failed: bool,
    ) -> Option<LogEvent> {
        let event = Event::from(event);
        let mut parser = RegexParserConfig {
            regex: regex.into(),
            field: field.map(|field| field.into()),
            drop_field,
            drop_failed,
        }
        .build()
        .unwrap();

        parser.transform(event).map(|event| event.into_log())
    }

    #[test]
    fn regex_parser_adds_parsed_field_to_event() {
        let log = do_transform(
            "status=1234 time=5678",
            r"status=(?P<status>\d+) time=(?P<time>\d+)",
            None,
            false,
            false,
        )
        .unwrap();

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn regex_parser_doesnt_do_anything_if_no_match() {
        let log = do_transform("asdf1234", r"status=(?P<status>\d+)", None, false, false).unwrap();

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
            false,
        )
        .unwrap();

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
            false,
        )
        .unwrap();

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"message".into()], "yes".into());
    }

    #[test]
    fn regex_parser_does_not_drop_field_if_no_match() {
        let log = do_transform(
            "asdf1234",
            r"status=(?P<message>\S+)",
            Some("message"),
            true,
            false,
        )
        .unwrap();

        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn regex_parser_does_not_drop_event_if_match() {
        let log = do_transform("asdf1234", r"asdf", None, false, true);
        assert!(log.is_some());
    }

    #[test]
    fn regex_parser_does_drop_event_if_no_match() {
        let log = do_transform("asdf1234", r"something", None, false, true);
        assert!(log.is_none());
    }
}
