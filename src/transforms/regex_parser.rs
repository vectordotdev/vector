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
    pub field: Option<String>,
}

#[typetag::serde(name = "regex_parser")]
impl crate::topology::config::TransformConfig for RegexParserConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        let field = self.field.clone();

        Regex::new(&self.regex)
            .map_err(|err| err.to_string())
            .map::<Box<dyn Transform>, _>(|r| Box::new(RegexParser::new(r, field)))
    }
}

pub struct RegexParser {
    regex: Regex,
    field: Option<Atom>,
}

impl RegexParser {
    pub fn new(regex: Regex, field: Option<String>) -> Self {
        Self {
            regex,
            field: field.map(Atom::from),
        }
    }
}

impl Transform for RegexParser {
    fn transform(&self, mut event: Event) -> Option<Event> {
        let field = if let Some(field) = &self.field {
            event.as_log()[&field].as_bytes().into_owned()
        } else {
            event.as_log()[&event::MESSAGE].as_bytes().into_owned()
        };

        if let Some(captures) = self.regex.captures(&field) {
            for name in self.regex.capture_names().filter_map(|c| c) {
                if let Some(capture) = captures.name(name) {
                    event
                        .as_mut_log()
                        .insert_explicit(name.into(), capture.as_bytes().into());
                }
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::RegexParser;
    use crate::transforms::Transform;
    use crate::Event;
    use regex::bytes::Regex;

    #[test]
    fn regex_parser_adds_parsed_field_to_event() {
        let event = Event::from("status=1234 time=5678");
        let parser = RegexParser::new(
            Regex::new(r"status=(?P<status>\d+) time=(?P<time>\d+)").unwrap(),
            None,
        );

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log()[&"status".into()], "1234".into());
        assert_eq!(event.as_log()[&"time".into()], "5678".into());
    }

    #[test]
    fn regex_parser_doesnt_do_anything_if_no_match() {
        let event = Event::from("asdf1234");
        let parser = RegexParser::new(Regex::new(r"status=(?P<status>\d+)").unwrap(), None);

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log().get(&"status".into()), None);
    }
}
