use super::Transform;
use crate::event::{self, Event};
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RegexParserConfig {
    pub regex: String,
    pub field: Option<Atom>,
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
            .map::<Box<dyn Transform>, _>(|r| Box::new(RegexParser::new(r, field.clone())))
    }
}

pub struct RegexParser {
    regex: Regex,
    field: Atom,
}

impl RegexParser {
    pub fn new(regex: Regex, field: Atom) -> Self {
        Self { regex, field }
    }
}

impl Transform for RegexParser {
    fn transform(&self, mut event: Event) -> Option<Event> {
        let value = event.as_log().get(&self.field).map(|s| s.as_bytes());

        if let Some(value) = &value {
            if let Some(captures) = self.regex.captures(&value) {
                for name in self.regex.capture_names().filter_map(|c| c) {
                    if let Some(capture) = captures.name(name) {
                        event
                            .as_mut_log()
                            .insert_explicit(name.into(), capture.as_bytes().into());
                    }
                }
            } else {
                debug!(message = "No fields captured from regex");
            }
        } else {
            debug!(
                message = "Field does not exist.",
                field = self.field.as_ref(),
            );
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::RegexParserConfig;
    use crate::{topology::config::TransformConfig, Event};

    #[test]
    fn regex_parser_adds_parsed_field_to_event() {
        let event = Event::from("status=1234 time=5678");
        let parser = RegexParserConfig {
            regex: r"status=(?P<status>\d+) time=(?P<time>\d+)".into(),
            field: None,
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log()[&"status".into()], "1234".into());
        assert_eq!(event.as_log()[&"time".into()], "5678".into());
    }

    #[test]
    fn regex_parser_doesnt_do_anything_if_no_match() {
        let event = Event::from("asdf1234");
        let parser = RegexParserConfig {
            regex: r"status=(?P<status>\d+)".into(),
            field: None,
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log().get(&"status".into()), None);
    }
}
