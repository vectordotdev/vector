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

#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct KeyValueConfig {
    pub separator: String,
    pub field_split: String,
    pub field: Option<Atom>,
    pub drop_field: bool,
    pub types: HashMap<Atom, String>,
}

inventory::submit! {
    TransformDescription::new::<KeyValueConfig>("kv_parser")
}

#[typetag::serde(name = "kv_parser")]
impl TransformConfig for KeyValueConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let field = self
            .field
            .as_ref()
            .unwrap_or(&event::log_schema().message_key());

        let conversions = parse_conversion_map(&self.types)?;

        Ok(Box::new(KeyValue::new(
            self.separator.clone(),
            self.field_split.clone(),
            field.clone(),
            self.drop_field,
            conversions,
        )))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "kv_parser"
    }
}

pub struct KeyValue {
    separator: String,
    field_split: String,
    field: Atom,
    drop_field: bool,
    conversions: HashMap<Atom, Conversion>,
}

impl KeyValue {
    pub fn new(
        separator: String,
        field_split: String,
        field: Atom,
        drop_field: bool,
        conversions: HashMap<Atom, Conversion>,
    ) -> Self {
        Self {
            separator,
            field_split,
            field,
            drop_field,
            conversions,
        }
    }
}

fn kv_parser(pair: String, field_split: &String) -> Option<(Atom, String)> {
    let pair = pair.trim();
    if field_split.is_empty() {
        let mut kv_pair = pair.split_whitespace();
        let key = kv_pair.nth(0)?.to_string();
        let value = kv_pair.nth(1)?.to_string();
        let count = kv_pair.count();

        if count < 2 {
            return None;
        } else if count > 2 {
            debug!(message = "KV parser expected 2 values, but got {count}", count=count)
        }

        Some((Atom::from(key), value))
    } else {
        let split_index = pair.find(field_split).unwrap_or(0);
        let (key, val) = pair.split_at(split_index);
        let key = key.trim();
        let val = val.trim_start_matches(field_split).trim();

        if key.is_empty() || val.is_empty() {
            return None;
        }

        Some((Atom::from(key), val.to_string()))
    }
}

impl Transform for KeyValue {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let value = event.as_log().get(&self.field).map(|s| s.to_string_lossy());

        if let Some(value) = &value {
            //let pairs = parse_kv(value, self.separator.copy(), self.field_split.copy());
            let pairs = value
                .split(&self.separator)
                .filter_map(|pair| kv_parser(pair.to_string(), &self.field_split));

            for (key, val) in pairs {
                if let Some(conv) = self.conversions.get(&key) {
                    match conv.convert(val.as_bytes().into()) {
                        Ok(value) => {
                            event.as_mut_log().insert(key, value);
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
                    event.as_mut_log().insert(key, val);
                }
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

#[cfg(test)]
mod tests {
    use super::KeyValueConfig;
    use crate::{
        event::{LogEvent, Value},
        topology::config::{TransformConfig, TransformContext},
        Event,
    };

    fn parse_log(
        text: &str,
        separator: String,
        field_split: String,
        drop_field: bool,
        types: &[(&str, &str)],
    ) -> LogEvent {
        let event = Event::from(text);

        let mut parser = KeyValueConfig {
            separator,
            field_split,
            field: None,
            drop_field,
            types: types.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
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
            false,
            &[],
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
        );
        assert!(log.contains(&"foo".into()));
        assert!(!log.contains(&"beep".into()));
        assert!(!log.contains(&"score".into()));
    }
}
