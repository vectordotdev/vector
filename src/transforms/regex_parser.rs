use super::Transform;
use crate::{
    config::{DataType, TransformConfig, TransformContext, TransformDescription},
    event::{self, Event, Value},
    internal_events::{RegexEventProcessed, RegexFailedMatch, RegexMissingField},
    types::{parse_check_conversion_map, Conversion},
};
use regex::bytes::{CaptureLocations, Regex, RegexSet};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::collections::HashMap;
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(default, deny_unknown_fields)]
pub struct RegexParserConfig {
    /// Deprecated. Use `patterns` instead.
    /// See #2469.
    /// TODO: Remove at a future point in time.
    pub regex: Option<String>,
    pub patterns: Vec<String>,
    pub field: Option<Atom>,
    #[derivative(Default(value = "true"))]
    pub drop_field: bool,
    pub drop_failed: bool,
    pub target_field: Option<Atom>,
    #[derivative(Default(value = "true"))]
    pub overwrite_target: bool,
    pub types: HashMap<Atom, String>,
}

inventory::submit! {
    TransformDescription::new::<RegexParserConfig>("regex_parser")
}

#[typetag::serde(name = "regex_parser")]
impl TransformConfig for RegexParserConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        RegexParser::build(&self)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "regex"
    }
}

pub struct RegexParser {
    regexset: RegexSet,
    patterns: Vec<CompiledRegex>, // indexes correspend to RegexSet
    field: Atom,
    drop_field: bool,
    drop_failed: bool,
    target_field: Option<Atom>,
    overwrite_target: bool,
}

struct CompiledRegex {
    regex: Regex,
    capture_names: Vec<(usize, Atom, Conversion)>,
    capture_locs: CaptureLocations,
}

impl CompiledRegex {
    fn new(regex: Regex, types: &HashMap<Atom, Conversion>) -> CompiledRegex {
        // Calculate the location (index into the capture locations) of
        // each named capture, and the required type coercion.
        let capture_names = regex
            .capture_names()
            .enumerate()
            .filter_map(|(idx, cn)| {
                cn.map(|cn| {
                    let cn = Atom::from(cn);
                    let conv = types.get(&cn).unwrap_or(&Conversion::Bytes);
                    (idx, cn, conv.clone())
                })
            })
            .collect::<Vec<_>>();
        let capture_locs = regex.capture_locations();
        CompiledRegex {
            regex,
            capture_names,
            capture_locs,
        }
    }

    /// Returns a list of captures (name, value) or None if the regex does not
    /// match
    fn captures<'a>(
        &'a mut self,
        value: &'a [u8],
    ) -> Option<impl Iterator<Item = (Atom, Value)> + 'a> {
        match self.regex.captures_read(&mut self.capture_locs, value) {
            Some(_) => {
                let capture_locs = &self.capture_locs;
                let values =
                    self.capture_names
                        .iter()
                        .filter_map(move |(idx, name, conversion)| {
                            capture_locs.get(*idx).and_then(|(start, end)| {
                                let capture: Value = value[start..end].into();

                                match conversion.convert(capture) {
                                    Ok(value) => Some((name.clone(), value)),
                                    Err(error) => {
                                        debug!(
                                            message = "Could not convert types.",
                                            name = &name[..],
                                            %error,
                                            rate_limit_secs = 30
                                        );
                                        None
                                    }
                                }
                            })
                        });
                Some(values)
            }
            None => None,
        }
    }
}

impl RegexParser {
    pub fn build(config: &RegexParserConfig) -> crate::Result<Box<dyn Transform>> {
        let field = config
            .field
            .as_ref()
            .unwrap_or(&event::log_schema().message_key());

        let patterns = match (&config.regex, &config.patterns.len()) {
            (None, 0) => {
                return Err(
                    "At least one regular expression must be defined, but `patterns` is empty"
                        .into(),
                );
            }
            (None, _) => config.patterns.clone(),
            (Some(regex), 0) => {
                // Still using the old `regex` syntax.
                // Printing a warning and wrapping input in a `vec`.
                warn!(
                    "Usage of `regex` is deprecated and will be removed in a future version. \
                     Please upgrade your config to use `patterns` instead: \
                     `patterns = ['{}']`. For more info, take a look at the documentation at \
                     https://vector.dev/docs/reference/transforms/regex_parser/",
                    &regex
                );
                vec![regex.clone()]
            }
            _ => {
                return Err("`patterns = [...]` is not defined".into());
            }
        };

        let regexset = RegexSet::new(&patterns).context(super::InvalidRegex)?;

        // Pre-compile individual patterns
        let patterns: Result<Vec<Regex>, _> = regexset
            .patterns()
            .iter()
            .map(|pattern| Regex::new(pattern))
            .collect();
        let patterns = patterns.context(super::InvalidRegex)?;

        let names = &patterns
            .iter()
            .map(|regex| {
                regex
                    .capture_names()
                    .filter_map(|s| s.map(Into::into))
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect::<Vec<_>>();

        let types = parse_check_conversion_map(&config.types, names)?;

        Ok(Box::new(RegexParser::new(
            regexset,
            patterns,
            field.clone(),
            config.drop_field,
            config.drop_failed,
            config.target_field.clone(),
            config.overwrite_target,
            types,
        )))
    }

    pub fn new(
        regexset: RegexSet,
        patterns: Vec<Regex>,
        field: Atom,
        mut drop_field: bool,
        drop_failed: bool,
        target_field: Option<Atom>,
        overwrite_target: bool,
        types: HashMap<Atom, Conversion>,
    ) -> Self {
        // Build a buffer of the regex capture locations and names to avoid
        // repeated allocations.
        let patterns: Vec<CompiledRegex> = patterns
            .into_iter()
            .map(|regex| CompiledRegex::new(regex, &types))
            .collect();

        // Pre-calculate if the source field name should be dropped.
        drop_field = drop_field
            && !patterns
                .iter()
                .map(|p| &p.capture_names)
                .flatten()
                .any(|(_, f, _)| *f == field);

        Self {
            regexset,
            patterns,
            field,
            drop_field,
            drop_failed,
            target_field,
            overwrite_target,
        }
    }
}

impl Transform for RegexParser {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();
        let value = log.get(&self.field).map(|s| s.as_bytes());
        emit!(RegexEventProcessed);

        if let Some(value) = &value {
            let regex_id = self.regexset.matches(&value).into_iter().next();
            let id = match regex_id {
                Some(id) => id,
                None => {
                    emit!(RegexFailedMatch { value });
                    if self.drop_failed {
                        return None;
                    } else {
                        return Some(event);
                    }
                }
            };

            let target_field = self.target_field.as_ref();

            let pattern = self
                .patterns
                .get_mut(id)
                .expect("Mismatch between capture patterns and regexset");

            if let Some(captures) = pattern.captures(&value) {
                // Handle optional overwriting of the target field
                if let Some(target_field) = target_field {
                    if log.contains(target_field) {
                        if self.overwrite_target {
                            log.remove(target_field);
                        } else {
                            error!(message = "Target field already exists", %target_field, rate_limit_secs = 30);
                            return Some(event);
                        }
                    }
                }

                log.extend(captures.map(|(name, value)| {
                    let name = target_field
                        .map(|target| Atom::from(format!("{}.{}", target, name)))
                        .unwrap_or_else(|| name.clone());
                    (name, value)
                }));
                if self.drop_field {
                    log.remove(&self.field);
                }
                return Some(event);
            } else {
                emit!(RegexFailedMatch { value });
            }
        } else {
            emit!(RegexMissingField { field: &self.field });
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
    use crate::event::{LogEvent, Value};
    use crate::{
        config::{TransformConfig, TransformContext},
        Event,
    };

    fn do_transform(event: &str, patterns: &str, config: &str) -> Option<LogEvent> {
        let event = Event::from(event);
        let mut parser = toml::from_str::<RegexParserConfig>(&format!(
            r#"
                patterns = {}
                {}
            "#,
            patterns, config
        ))
        .unwrap()
        .build(TransformContext::new_test())
        .unwrap();

        parser.transform(event).map(|event| event.into_log())
    }

    #[test]
    fn adds_parsed_field_to_event() {
        let log = do_transform(
            "status=1234 time=5678",
            r#"['status=(?P<status>\d+) time=(?P<time>\d+)']"#,
            "drop_field = false",
        )
        .unwrap();

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn doesnt_do_anything_if_no_match() {
        let log = do_transform(
            "asdf1234",
            r#"['status=(?P<status>\d+)']"#,
            "drop_field = false",
        )
        .unwrap();

        assert_eq!(log.get(&"status".into()), None);
        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn does_drop_parsed_field() {
        let log = do_transform(
            "status=1234 time=5678",
            r#"['status=(?P<status>\d+) time=(?P<time>\d+)']"#,
            r#"field = "message""#,
        )
        .unwrap();

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"time".into()], "5678".into());
        assert!(log.get(&"message".into()).is_none());
    }

    #[test]
    fn does_not_drop_same_name_parsed_field() {
        let log = do_transform(
            "status=1234 message=yes",
            r#"['status=(?P<status>\d+) message=(?P<message>\S+)']"#,
            r#"field = "message""#,
        )
        .unwrap();

        assert_eq!(log[&"status".into()], "1234".into());
        assert_eq!(log[&"message".into()], "yes".into());
    }

    #[test]
    fn does_not_drop_field_if_no_match() {
        let log = do_transform(
            "asdf1234",
            r#"['status=(?P<message>\S+)']"#,
            r#"field = "message""#,
        )
        .unwrap();

        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    fn respects_target_field() {
        let mut log = do_transform(
            "status=1234 time=5678",
            r#"['status=(?P<status>\d+) time=(?P<time>\d+)']"#,
            r#"
               target_field = "prefix"
               drop_field = false
            "#,
        )
        .unwrap();

        // timestamp is unpredictable, don't compare it
        log.remove(&"timestamp".into());
        let log = serde_json::to_value(log.all_fields()).unwrap();
        assert_eq!(
            log,
            serde_json::json!({
                "message": "status=1234 time=5678",
                "prefix.status": "1234",
                "prefix.time": "5678",
            })
        );
    }

    #[test]
    fn preserves_target_field() {
        let message = "status=1234 time=5678";
        let log = do_transform(
            message,
            r#"['status=(?P<status>\d+) time=(?P<time>\d+)']"#,
            r#"
               target_field = "message"
               overwrite_target = false
            "#,
        )
        .unwrap();

        assert_eq!(log[&"message".into()], message.into());
        assert_eq!(log.get(&"message.status".into()), None);
        assert_eq!(log.get(&"message.time".into()), None);
    }

    #[test]
    fn overwrites_target_field() {
        let mut log = do_transform(
            "status=1234 time=5678",
            r#"['status=(?P<status>\d+) time=(?P<time>\d+)']"#,
            r#"
               target_field = "message"
               drop_field = false
            "#,
        )
        .unwrap();

        // timestamp is unpredictable, don't compare it
        log.remove(&"timestamp".into());
        let log = serde_json::to_value(log.all_fields()).unwrap();
        assert_eq!(
            log,
            serde_json::json!({
                "message.status": "1234",
                "message.time": "5678",
            })
        );
    }

    #[test]
    fn does_not_drop_event_if_match() {
        let log = do_transform("asdf1234", r#"['asdf']"#, "drop_failed = true");
        assert!(log.is_some());
    }

    #[test]
    fn does_drop_event_if_no_match() {
        let log = do_transform("asdf1234", r#"['something']"#, "drop_failed = true");
        assert!(log.is_none());
    }

    #[test]
    fn handles_valid_optional_capture() {
        let log = do_transform("1234", r#"['(?P<status>\d+)?']"#, "").unwrap();
        assert_eq!(log[&"status".into()], "1234".into());
    }

    #[test]
    fn handles_missing_optional_capture() {
        let log = do_transform("none", r#"['(?P<status>\d+)?']"#, "").unwrap();
        assert!(log.get(&"status".into()).is_none());
    }

    #[test]
    fn coerces_fields_to_types() {
        let log = do_transform(
            "1234 6789.01 false",
            r#"['(?P<status>\d+) (?P<time>[\d.]+) (?P<check>\S+)']"#,
            r#"
            [types]
            status = "int"
            time = "float"
            check = "boolean"
            "#,
        )
        .expect("Failed to parse log");
        assert_eq!(log[&"check".into()], Value::Boolean(false));
        assert_eq!(log[&"status".into()], Value::Integer(1234));
        assert_eq!(log[&"time".into()], Value::Float(6789.01));
    }

    #[test]
    fn chooses_first_of_multiple_matching_patterns() {
        let log = do_transform(
            "1234 235.42 true",
            r#"[
                '^(?P<id1>\d+)',
                '^(?P<id2>\d+) (?P<time>[\d.]+) (?P<check>\S+)$',
            ]"#,
            r#"
            drop_field = false
            [types]
            id1 = "int"
            id2 = "int"
            time = "float"
            check = "boolean"
            "#,
        )
        .unwrap();

        assert_eq!(log[&"id1".into()], Value::Integer(1234));
        assert_eq!(log.get(&"id2".into()), None);
        assert_eq!(log.get(&"time".into()), None);
        assert_eq!(log.get(&"check".into()), None);
        assert!(log.get(&"message".into()).is_some());
    }

    #[test]
    // https://github.com/timberio/vector/issues/3096
    fn correctly_maps_capture_groups_if_matching_pattern_is_not_first() {
        let log = do_transform(
            "match1234 235.42 true",
            r#"[
                '^nomatch(?P<id1>\d+)$',
                '^match(?P<id2>\d+) (?P<time>[\d.]+) (?P<check>\S+)$',
            ]"#,
            r#"
            drop_field = false
            [types]
            id1 = "int"
            id2 = "int"
            time = "float"
            check = "boolean"
            "#,
        )
        .unwrap();

        assert_eq!(log.get(&"id1".into()), None);
        assert_eq!(log[&"id2".into()], Value::Integer(1234));
        assert_eq!(log[&"time".into()], Value::Float(235.42));
        assert_eq!(log[&"check".into()], Value::Boolean(true));
        assert!(log.get(&"message".into()).is_some());
    }
}
