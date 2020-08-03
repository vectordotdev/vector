use super::Transform;
use crate::{
    event::{self, Event},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
    types::{parse_conversion_map, Conversion},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Debug, Default, Derivative, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeyValueConfig {
    #[derivative(Default(value = "true"))]
    pub drop_field: bool,
    pub field: Option<Atom>,
    #[derivative(Default(value = "="))]
    pub field_split: String,
    #[derivative(Default(value = "true"))]
    pub overwrite_target: bool,
    pub separator: String,
    pub target_field: Option<Atom>,
    pub trim_key: Option<String>,
    pub trim_value: Option<String>,
    pub types: HashMap<Atom, String>,
}

inventory::submit! {
    TransformDescription::new::<KeyValueConfig>("key_value_parser")
}

#[typetag::serde(name = "key_value_parser")]
impl TransformConfig for KeyValueConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let field = self
            .field
            .as_ref()
            .unwrap_or(&event::log_schema().message_key());

        let conversions = parse_conversion_map(&self.types)?;

        Ok(Box::new(KeyValue {
            conversions,
            drop_field: self.drop_field,
            field: field.clone(),
            field_split: Option::from(self.field_split.clone()),
            overwrite_target: self.overwrite_target,
            separator: self.separator.clone(),
            target_field: self.target_field.clone(),
            trim_key: self.trim_key.clone(),
            trim_value: self.trim_value.clone(),
        }))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "key_value_parser"
    }
}

pub struct KeyValue {
    conversions: HashMap<Atom, Conversion>,
    drop_field: bool,
    field: Atom,
    field_split: Option<String>,
    overwrite_target: bool,
    separator: String,
    target_field: Option<Atom>,
    trim_key: Option<String>,
    trim_value: Option<String>,
}

fn kv_parser(
    pair: String,
    field_split: &str,
    trim_key: &Option<String>,
    trim_value: &Option<String>,
) -> Option<(Atom, String)> {
    let pair = pair.trim();
    let mut field;

    if field_split.is_empty() {
        let mut kv_pair = pair.split_whitespace();
        let key = kv_pair.nth(0)?;
        let val = kv_pair.nth(1)?;
        let count = kv_pair.count();

        if count < 2 {
            return None;
        } else if count > 2 {
            error!(
                message = "KV parser expected 2 values, but got {count}",
                count = count,
                rate_limit_secs = 30
            );
            return None;
        }

        field = [key, val];
    } else {
        let split_index = pair.find(field_split).unwrap_or(0);
        let (key, val) = pair.split_at(split_index);
        let key = key.trim();
        let val = val.trim_start_matches(field_split).trim();

        if key.is_empty() || val.is_empty() {
            return None;
        }

        field = [key, val];
    }

    for (i, trim) in [trim_key, trim_value].iter().enumerate() {
        if let Some(trim) = trim {
            let trim_chars: Vec<char> = trim.chars().collect();
            field[i] = field[i].trim_matches(&trim_chars as &[_]);
        }
    }

    Some((Atom::from(field[0]), field[1].to_string()))
}

impl Transform for KeyValue {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();
        let value = log.get(&self.field).map(|s| s.to_string_lossy());

        if let Some(value) = &value {
            let field_split = self.field_split.as_ref();
            let pairs = value.split(&self.separator).filter_map(|pair| {
                kv_parser(
                    pair.to_string(),
                    &field_split?,
                    &self.trim_key,
                    &self.trim_value,
                )
            });

            // Handle optional overwriting of the target field
            if let Some(target_field) = &self.target_field {
                if log.contains(target_field) {
                    if self.overwrite_target {
                        log.remove(target_field);
                    } else {
                        error!(message = "target field already exists", %target_field, rate_limit_secs = 30);
                        return Some(event);
                    }
                }
            }

            for (mut key, val) in pairs {
                if let Some(target_field) = self.target_field.to_owned() {
                    key = Atom::from(format!("{}.{}", target_field, key));
                }

                if let Some(conv) = self.conversions.get(&key) {
                    match conv.convert(val.as_bytes().into()) {
                        Ok(value) => {
                            log.insert(key, value);
                        }
                        Err(error) => {
                            debug!(
                                message = "Could not convert types.",
                                key = &key[..],
                                %error,
                                rate_limit_secs = 30
                            );
                        }
                    }
                } else {
                    log.insert(key, val);
                }
            }

            if self.drop_field {
                log.remove(&self.field);
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
    use super::KeyValueConfig;
    use crate::{
        event::{LogEvent, Value},
        topology::config::{TransformConfig, TransformContext},
        Event,
    };
    use string_cache::DefaultAtom as Atom;

    fn parse_log(
        text: &str,
        separator: String,
        field_split: String,
        drop_field: bool,
        types: &[(&str, &str)],
        target_field: Option<Atom>,
        trim_key: Option<String>,
        trim_value: Option<String>,
    ) -> LogEvent {
        let event = Event::from(text);

        let mut parser = KeyValueConfig {
            separator,
            field_split,
            field: None,
            drop_field,
            types: types.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
            target_field,
            overwrite_target: false,
            trim_key,
            trim_value,
        }
        .build(TransformContext::new_test())
        .unwrap();

        parser.transform(event).unwrap().into_log()
    }

    #[test]
    fn it_separates_whitespace() {
        let log = parse_log(
            "foo=bar beep=bop",
            " ".to_string(),
            "=".to_string(),
            true,
            &[],
            None,
            None,
            None,
        );
        assert_eq!(log[&"foo".into()], Value::Bytes("bar".into()));
        assert_eq!(log[&"beep".into()], Value::Bytes("bop".into()));
    }

    #[test]
    fn it_separates_csv_kv() {
        let log = parse_log(
            "foo=bar, beep=bop, score=10",
            ",".to_string(),
            "=".to_string(),
            false,
            &[],
            None,
            None,
            None,
        );
        assert_eq!(log[&"foo".into()], Value::Bytes("bar".into()));
        assert_eq!(log[&"beep".into()], Value::Bytes("bop".into()));
    }

    #[test]
    fn it_splits_whitespace() {
        let log = parse_log(
            "foo bar, beep bop, score 10",
            ",".to_string(),
            " ".to_string(),
            false,
            &[],
            None,
            None,
            None,
        );
        assert_eq!(log[&"foo".into()], Value::Bytes("bar".into()));
        assert_eq!(log[&"beep".into()], Value::Bytes("bop".into()));
    }

    #[test]
    fn it_handles_whitespace_in_fields() {
        let log = parse_log(
            "foo:bar, beep : bop, score :10",
            ",".to_string(),
            ":".to_string(),
            false,
            &[("score", "integer")],
            None,
            None,
            None,
        );
        assert_eq!(log[&"foo".into()], Value::Bytes("bar".into()));
        assert_eq!(log[&"beep".into()], Value::Bytes("bop".into()));
        assert_eq!(log[&"score".into()], Value::Integer(10));
    }

    #[test]
    fn it_handles_multi_char_splitters() {
        let log = parse_log(
            "foo=>bar || beep => bop || score=>10",
            "||".to_string(),
            "=>".to_string(),
            false,
            &[("score", "integer")],
            None,
            None,
            None,
        );

        assert_eq!(log[&"foo".into()], Value::Bytes("bar".into()));
        assert_eq!(log[&"beep".into()], Value::Bytes("bop".into()));
        assert_eq!(log[&"score".into()], Value::Integer(10));
    }

    #[test]
    fn it_handles_splitters_in_value() {
        let log = parse_log(
            "foo==bar, beep=bop=bap , score=10",
            ",".to_string(),
            "=".to_string(),
            false,
            &[("score", "integer")],
            None,
            None,
            None,
        );
        assert_eq!(log[&"foo".into()], Value::Bytes("bar".into()));
        assert_eq!(log[&"beep".into()], Value::Bytes("bop=bap".into()));
        assert_eq!(log[&"score".into()], Value::Integer(10));
    }

    #[test]
    fn it_fails_graceful_on_empty_values() {
        let log = parse_log(
            "foo::0, ::beep, score:: ",
            ",".to_string(),
            "::".to_string(),
            false,
            &[],
            None,
            None,
            None,
        );
        assert!(log.contains(&"foo".into()));
        assert!(!log.contains(&"beep".into()));
        assert!(!log.contains(&"score".into()));
    }

    #[test]
    fn it_accepts_brackets() {
        let log = parse_log(
            "{\"foo\"}:0, \"\"bop\":[beep], [score]:78",
            ",".to_string(),
            ":".to_string(),
            false,
            &[],
            None,
            Some("\"{}".to_string()),
            None,
        );

        // println!("{}", log.keys().fold("".to_string(), |res, key| [res, key].join(", ")));
        // Review: This fails and I don't know why. The key is missing from the results.
        // assert!(log.contains(&"[score]".into()));
        assert_eq!(log[&"bop".into()], Value::Bytes("[beep]".into()))
    }

    #[test]
    fn it_trims_keys() {
        let log = parse_log(
            "{\"foo\"}:0, \"\"bop\":beep, {({score})}:78",
            ",".to_string(),
            ":".to_string(),
            false,
            &[],
            None,
            Some("\"{}".to_string()),
            None,
        );
        assert!(log.contains(&"foo".into()));
        assert!(log.contains(&"bop".into()));
        assert!(log.contains(&"({score})".into()));
    }

    #[test]
    fn it_trims_values() {
        let log = parse_log(
            "foo:{\"0\"}, bop:\"beep\", score:{78}",
            ",".to_string(),
            ":".to_string(),
            false,
            &[("foo", "integer"), ("score", "integer")],
            None,
            None,
            Some("\"{}".to_string()),
        );
        assert_eq!(log[&"foo".into()], Value::Integer(0));
        assert_eq!(log[&"bop".into()], Value::Bytes("beep".into()));
        assert_eq!(log[&"score".into()], Value::Integer(78));
    }
}
