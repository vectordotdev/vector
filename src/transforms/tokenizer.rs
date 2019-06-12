use super::Transform;
use crate::event::{self, Event};
use nom::{
    branch::alt,
    bytes::complete::{escaped, is_not, tag},
    character::complete::{one_of, space0},
    combinator::{all_consuming, rest, verify},
    multi::many0,
    sequence::{delimited, terminated},
};
use serde::{Deserialize, Serialize};
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct TokenizerConfig {
    pub field_names: Vec<Atom>,
    pub field: Option<Atom>,
    pub drop_field: bool,
}

#[typetag::serde(name = "tokenizer")]
impl crate::topology::config::TransformConfig for TokenizerConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        let field = if let Some(field) = &self.field {
            field
        } else {
            &event::MESSAGE
        };

        // don't drop the source field if it's getting overwritten by a parsed value
        let drop_field = self.drop_field && !self.field_names.iter().any(|f| f == field);

        Ok(Box::new(Tokenizer::new(
            self.field_names.clone(),
            field.clone(),
            drop_field,
        )))
    }
}

pub struct Tokenizer {
    field_names: Vec<Atom>,
    field: Atom,
    drop_field: bool,
}

impl Tokenizer {
    pub fn new(field_names: Vec<Atom>, field: Atom, drop_field: bool) -> Self {
        Self {
            field_names,
            field,
            drop_field,
        }
    }
}

impl Transform for Tokenizer {
    fn transform(&self, mut event: Event) -> Option<Event> {
        let value = event.as_log().get(&self.field).map(|s| s.to_string_lossy());

        if let Some(value) = &value {
            for (name, value) in self.field_names.iter().zip(parse(value).into_iter()) {
                event
                    .as_mut_log()
                    .insert_explicit(name.clone(), value.as_bytes().into());
            }
            if self.drop_field {
                event.as_mut_log().remove(&self.field);
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

pub fn parse(input: &str) -> Vec<&str> {
    let simple = is_not::<_, _, (&str, nom::error::ErrorKind)>(" \t[\"");
    let string = delimited(
        tag("\""),
        escaped(is_not("\"\\"), '\\', one_of("\"\\")),
        tag("\""),
    );
    let bracket = delimited(
        tag("["),
        escaped(is_not("]\\"), '\\', one_of("]\\")),
        tag("]"),
    );

    // fall back to returning the rest of the input, if any
    let remainder = verify(rest, |s: &str| s.len() > 0);
    let field = alt((bracket, string, simple, remainder));

    all_consuming(many0(terminated(field, space0)))(input)
        .expect("parser should always succeed")
        .1
}

#[cfg(test)]
mod tests {
    use super::parse;
    use super::TokenizerConfig;
    use crate::{topology::config::TransformConfig, Event};

    #[test]
    fn basic() {
        assert_eq!(parse("foo"), &["foo"]);
    }

    #[test]
    fn multiple() {
        assert_eq!(parse("foo bar"), &["foo", "bar"]);
    }

    #[test]
    fn more_space() {
        assert_eq!(parse("foo\t bar"), &["foo", "bar"]);
    }

    #[test]
    fn quotes() {
        assert_eq!(parse(r#"foo "bar baz""#), &["foo", r#"bar baz"#]);
    }

    #[test]
    fn escaped_quotes() {
        assert_eq!(
            parse(r#"foo "bar \" \" baz""#),
            &["foo", r#"bar \" \" baz"#],
        );
    }

    #[test]
    fn unclosed_quotes() {
        assert_eq!(parse(r#"foo "bar"#), &["foo", "\"bar"],);
    }

    #[test]
    fn brackets() {
        assert_eq!(parse("[foo.bar = baz] quux"), &["foo.bar = baz", "quux"],);
    }

    #[test]
    fn escaped_brackets() {
        assert_eq!(
            parse(r#"[foo " [[ \] "" bar] baz"#),
            &[r#"foo " [[ \] "" bar"#, "baz"],
        );
    }

    #[test]
    fn unclosed_brackets() {
        assert_eq!(parse("foo [bar"), &["foo", "[bar"],);
    }

    #[test]
    fn truncated_field() {
        assert_eq!(
            parse("foo bar[baz]: quux"),
            &["foo", "bar", "baz", ":", "quux"]
        );
        assert_eq!(parse("foo bar[baz quux"), &["foo", "bar", "[baz quux"]);
    }

    #[test]
    fn from_fuzzing() {
        assert_eq!(parse("").len(), 0);
        assert_eq!(parse("f] bar"), &["f]", "bar"]);
        assert_eq!(parse("f\" bar"), &["f", "\" bar"]);
        assert_eq!(parse("f[f bar"), &["f", "[f bar"]);
        assert_eq!(parse("f\"f bar"), &["f", "\"f bar"]);
        assert_eq!(parse("[][x"), &["", "[x"]);
        assert_eq!(parse("x[][x"), &["x", "", "[x"]);
    }

    #[test]
    fn tokenizer_adds_parsed_field_to_event() {
        let event = Event::from("1234 5678");
        let parser = TokenizerConfig {
            field_names: vec!["status".into(), "time".into()],
            field: None,
            ..Default::default()
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log()[&"status".into()], "1234".into());
        assert_eq!(event.as_log()[&"time".into()], "5678".into());
        assert!(event.as_log().get(&"message".into()).is_some());
    }

    #[test]
    fn tokenizer_does_drop_parsed_field() {
        let event = Event::from("1234 5678");
        let parser = TokenizerConfig {
            field_names: vec!["status".into(), "time".into()],
            field: Some("message".into()),
            drop_field: true,
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap();

        let log = event.into_log();
        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_none());
    }

    #[test]
    fn tokenizer_does_not_drop_same_name_parsed_field() {
        let event = Event::from("1234 yes");
        let parser = TokenizerConfig {
            field_names: vec!["status".into(), "message".into()],
            field: Some("message".into()),
            drop_field: true,
        }
        .build()
        .unwrap();

        let event = parser.transform(event).unwrap();

        let log = event.into_log();
        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"message".into()], "yes".into());
    }
}
