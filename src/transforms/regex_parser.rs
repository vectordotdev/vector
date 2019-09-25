use super::Transform;
use crate::{
    event::{self, Event, ValueKind},
    topology::config::{DataType, TransformConfig},
    types::{parse_check_conversion_map, Conversion},
};
use regex::bytes::{CaptureLocations, Regex};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::borrow::Cow;
use std::collections::HashMap;
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct RegexParserConfig {
    pub regex: String,
    pub field: Option<Atom>,
    pub drop_field: bool,
    pub drop_failed: bool,
    pub types: HashMap<Atom, String>,
}

#[typetag::serde(name = "regex_parser")]
impl TransformConfig for RegexParserConfig {
    fn build(&self) -> crate::Result<Box<dyn Transform>> {
        let field = self.field.as_ref().unwrap_or(&event::MESSAGE);

        let regex = Regex::new(&self.regex).context(super::InvalidRegex)?;

        let types = parse_check_conversion_map(
            &self.types,
            &regex
                .capture_names()
                .filter_map(|s| s.map(|s| s.into()))
                .collect(),
        )?;

        Ok(Box::new(RegexParser::new(
            regex,
            field.clone(),
            self.drop_field,
            self.drop_failed,
            types,
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

pub struct RegexParser {
    regex: Regex,
    field: Atom,
    drop_field: bool,
    drop_failed: bool,
    capture_names: Vec<(usize, Atom, Conversion)>,
    capture_locs: CaptureLocations,
}

impl RegexParser {
    pub fn new(
        regex: Regex,
        field: Atom,
        mut drop_field: bool,
        drop_failed: bool,
        types: HashMap<Atom, Conversion>,
    ) -> Self {
        // Build a buffer of the regex capture locations to avoid
        // repeated allocations.
        let capture_locs = regex.capture_locations();

        // Calculate the location (index into the capture locations) of
        // each named capture, and the required type coercion.
        let capture_names: Vec<(usize, Atom, Conversion)> = regex
            .capture_names()
            .enumerate()
            .filter_map(|(idx, cn)| {
                cn.map(|cn| {
                    let cn: Atom = cn.into();
                    let conv = types.get(&cn).unwrap_or(&Conversion::Bytes);
                    (idx, cn, conv.clone())
                })
            })
            .collect();

        // Pre-calculate if the source field name should be dropped.
        drop_field = drop_field && !capture_names.iter().any(|(_, f, _)| *f == field);

        Self {
            regex,
            field,
            drop_field,
            drop_failed,
            capture_names,
            capture_locs,
        }
    }
}

impl Transform for RegexParser {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let value = event.as_log().get(&self.field).map(|s| s.as_bytes());

        if let Some(value) = &value {
            if let Some(_) = self.regex.captures_read(&mut self.capture_locs, &value) {
                for (idx, name, conversion) in &self.capture_names {
                    if let Some((start, end)) = self.capture_locs.get(*idx) {
                        let capture: ValueKind = value[start..end].into();
                        match conversion.convert(capture) {
                            Ok(value) => event.as_mut_log().insert_explicit(name.clone(), value),
                            Err(error) => {
                                debug!(
                                    message = "Could not convert types.",
                                    name = &name[..],
                                    %error
                                );
                            }
                        }
                    }
                }
                if self.drop_field {
                    event.as_mut_log().remove(&self.field);
                }
                return Some(event);
            } else {
                warn!(
                    message = "Regex pattern failed to match.",
                    field = &truncate_string_at(&String::from_utf8_lossy(&value), 60)[..],
                );
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

fn truncate_string_at<'a>(s: &'a str, maxlen: usize) -> Cow<'a, str> {
    if s.len() >= maxlen {
        format!("{}[...]", &s[..maxlen - 5]).into()
    } else {
        s.into()
    }
}

#[cfg(test)]
mod tests {
    use super::RegexParserConfig;
    use crate::event::{LogEvent, ValueKind};
    use crate::{topology::config::TransformConfig, Event};

    fn do_transform(
        event: &str,
        regex: &str,
        field: Option<&str>,
        drop_field: bool,
        drop_failed: bool,
        types: &[(&str, &str)],
    ) -> Option<LogEvent> {
        let event = Event::from(event);
        let mut parser = RegexParserConfig {
            regex: regex.into(),
            field: field.map(|field| field.into()),
            drop_field,
            drop_failed,
            types: types.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
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
            &[],
        )
        .unwrap();

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn regex_parser_doesnt_do_anything_if_no_match() {
        let log = do_transform(
            "asdf1234",
            r"status=(?P<status>\d+)",
            None,
            false,
            false,
            &[],
        )
        .unwrap();

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
            &[],
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
            &[],
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
            &[],
        )
        .unwrap();

        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn regex_parser_does_not_drop_event_if_match() {
        let log = do_transform("asdf1234", r"asdf", None, false, true, &[]);
        assert!(log.is_some());
    }

    #[test]
    fn regex_parser_does_drop_event_if_no_match() {
        let log = do_transform("asdf1234", r"something", None, false, true, &[]);
        assert!(log.is_none());
    }

    #[test]
    fn regex_parser_handles_valid_optional_capture() {
        let log = do_transform("1234", r"(?P<status>\d+)?", None, false, false, &[]).unwrap();
        assert_eq!(log[&"status".into()], "1234".into());
    }

    #[test]
    fn regex_parser_handles_missing_optional_capture() {
        let log = do_transform("none", r"(?P<status>\d+)?", None, false, false, &[]).unwrap();
        assert!(log.get(&"status".into()).is_none());
    }

    #[test]
    fn regex_parser_coerces_fields_to_types() {
        let log = do_transform(
            "1234 6789.01 false",
            r"(?P<status>\d+) (?P<time>[\d.]+) (?P<check>\S+)",
            None,
            false,
            false,
            &[("status", "int"), ("time", "float"), ("check", "boolean")],
        )
        .expect("Failed to parse log");
        assert_eq!(log[&"check".into()], ValueKind::Boolean(false));
        assert_eq!(log[&"status".into()], ValueKind::Integer(1234));
        assert_eq!(log[&"time".into()], ValueKind::Float(6789.01));
    }
}
