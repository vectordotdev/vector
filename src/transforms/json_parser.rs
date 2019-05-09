use super::Transform;
use crate::event::{self, Event};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct JsonParserConfig {
    pub field: Atom,
    pub drop_invalid: bool,
}

impl Default for JsonParserConfig {
    fn default() -> Self {
        Self {
            field: event::MESSAGE.clone(),
            drop_invalid: false,
        }
    }
}

#[typetag::serde(name = "json_parser")]
impl crate::topology::config::TransformConfig for JsonParserConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        Ok(Box::new(JsonParser::from(self.clone())))
    }
}

struct JsonParser {
    config: JsonParserConfig,
}

impl From<JsonParserConfig> for JsonParser {
    fn from(config: JsonParserConfig) -> JsonParser {
        JsonParser { config }
    }
}

impl Transform for JsonParser {
    fn transform(&self, mut event: Event) -> Option<Event> {
        let to_parse = event.as_log().get(&self.config.field).map(|s| s.as_bytes());

        let parsed = to_parse
            .and_then(|to_parse| serde_json::from_slice::<Value>(to_parse.as_ref()).ok())
            .and_then(|value| {
                if let Value::Object(object) = value {
                    Some(object)
                } else {
                    None
                }
            });

        if let Some(object) = parsed {
            for (name, value) in object {
                insert(&mut event, name, value);
            }
        } else {
            if self.config.drop_invalid {
                return None;
            }
        }

        Some(event)
    }
}

fn insert(event: &mut Event, name: String, value: Value) {
    match value {
        Value::String(string) => {
            event
                .as_mut_log()
                .insert_explicit(name.into(), string.into());
        }
        Value::Number(number) => {
            event
                .as_mut_log()
                .insert_explicit(name.into(), number.to_string().into());
        }
        Value::Bool(b) => {
            event
                .as_mut_log()
                .insert_explicit(name.into(), b.to_string().into());
        }
        Value::Null => {
            event.as_mut_log().insert_explicit(name.into(), "".into());
        }
        Value::Array(array) => {
            for (i, element) in array.into_iter().enumerate() {
                let element_name = format!("{}[{}]", name, i);
                insert(event, element_name, element);
            }
        }
        Value::Object(object) => {
            for (key, value) in object.into_iter() {
                let item_name = format!("{}.{}", name, key);
                insert(event, item_name, value);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{JsonParser, JsonParserConfig};
    use crate::event::{self, Event};
    use crate::transforms::Transform;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn json_parser_parse_raw() {
        let parser = JsonParser::from(JsonParserConfig {
            ..Default::default()
        });

        let event = Event::from(r#"{"greeting": "hello", "name": "bob"}"#);

        let event = parser.transform(event).unwrap();

        assert_eq!(event[&Atom::from("greeting")], "hello".into());
        assert_eq!(event[&Atom::from("name")], "bob".into());
        assert_eq!(
            event[&event::MESSAGE],
            r#"{"greeting": "hello", "name": "bob"}"#.into()
        );
    }

    #[test]
    fn json_parser_parse_field() {
        let parser = JsonParser::from(JsonParserConfig {
            field: "data".into(),
            ..Default::default()
        });

        // Field present

        let mut event = Event::from("message");
        event.as_mut_log().insert_explicit(
            "data".into(),
            r#"{"greeting": "hello", "name": "bob"}"#.into(),
        );

        let event = parser.transform(event).unwrap();

        assert_eq!(event[&Atom::from("greeting")], "hello".into(),);
        assert_eq!(event[&Atom::from("name")], "bob".into());
        assert_eq!(
            event[&Atom::from("data")],
            r#"{"greeting": "hello", "name": "bob"}"#.into()
        );

        // Field missing
        let event = Event::from("message");

        let parsed = parser.transform(event.clone()).unwrap();

        assert_eq!(event, parsed);
    }

    #[test]
    fn json_parser_invalid_json() {
        let invalid = r#"{"greeting": "hello","#;

        // Raw
        let parser = JsonParser::from(JsonParserConfig {
            ..Default::default()
        });

        let event = Event::from(invalid);

        let parsed = parser.transform(event.clone()).unwrap();

        assert_eq!(event, parsed);
        assert_eq!(event[&event::MESSAGE], invalid.into());

        // Field
        let parser = JsonParser::from(JsonParserConfig {
            field: "data".into(),
            ..Default::default()
        });

        let mut event = Event::from("message");
        event
            .as_mut_log()
            .insert_explicit("data".into(), invalid.into());

        let event = parser.transform(event).unwrap();

        assert_eq!(event[&Atom::from("data")], invalid.into());
        assert!(event.as_log().get(&Atom::from("greeting")).is_none());
    }

    #[test]
    fn json_parser_drop_invalid() {
        let valid = r#"{"greeting": "hello", "name": "bob"}"#;
        let invalid = r#"{"greeting": "hello","#;
        let not_object = r#""hello""#;

        // Raw
        let parser = JsonParser::from(JsonParserConfig {
            drop_invalid: true,
            ..Default::default()
        });

        let event = Event::from(valid);
        assert!(parser.transform(event).is_some());

        let event = Event::from(invalid);
        assert!(parser.transform(event).is_none());

        let event = Event::from(not_object);
        assert!(parser.transform(event).is_none());

        // Field
        let parser = JsonParser::from(JsonParserConfig {
            field: "data".into(),
            drop_invalid: true,
            ..Default::default()
        });

        let mut event = Event::from("message");
        event
            .as_mut_log()
            .insert_explicit("data".into(), valid.into());
        assert!(parser.transform(event).is_some());

        let mut event = Event::from("message");
        event
            .as_mut_log()
            .insert_explicit("data".into(), invalid.into());
        assert!(parser.transform(event).is_none());

        let mut event = Event::from("message");
        event
            .as_mut_log()
            .insert_explicit("data".into(), not_object.into());
        assert!(parser.transform(event).is_none());

        // Missing field
        let event = Event::from("message");
        assert!(parser.transform(event).is_none());
    }

    #[test]
    fn json_parser_chained() {
        let parser1 = JsonParser::from(JsonParserConfig {
            ..Default::default()
        });
        let parser2 = JsonParser::from(JsonParserConfig {
            field: "nested".into(),
            ..Default::default()
        });

        let event = Event::from(r#"{"greeting": "hello", "name": "bob", "nested": "{\"message\": \"help i'm trapped under many layers of json\"}"}"#);
        let event = parser1.transform(event).unwrap();
        let event = parser2.transform(event).unwrap();

        assert_eq!(event[&Atom::from("greeting")], "hello".into());
        assert_eq!(event[&Atom::from("name")], "bob".into());
        assert_eq!(
            event[&Atom::from("message")],
            "help i'm trapped under many layers of json".into()
        );
    }

    #[test]
    fn json_parser_types() {
        let parser = JsonParser::from(JsonParserConfig {
            ..Default::default()
        });

        let event = Event::from(
            r#"{
              "string": "this is text",
              "null": null,
              "float": 12.34,
              "int": 56,
              "bool true": true,
              "bool false": false,
              "array": ["z", 7],
              "object": { "nested": "data", "more": "values" },
              "deep": [[[{"a": { "b": { "c": [[[1234]]]}}}]]]
            }"#,
        );
        let event = parser.transform(event).unwrap();

        assert_eq!(event[&Atom::from("string")], "this is text".into());
        assert_eq!(event[&Atom::from("null")], "".into());
        assert_eq!(event[&Atom::from("float")], "12.34".into());
        assert_eq!(event[&Atom::from("int")], "56".into());
        assert_eq!(event[&Atom::from("bool true")], "true".into());
        assert_eq!(event[&Atom::from("bool false")], "false".into());
        assert_eq!(event[&Atom::from("array[0]")], "z".into());
        assert_eq!(event[&Atom::from("array[1]")], "7".into());
        assert_eq!(event[&Atom::from("object.nested")], "data".into());
        assert_eq!(event[&Atom::from("object.more")], "values".into());
        assert_eq!(
            event[&Atom::from("deep[0][0][0].a.b.c[0][0][0]")],
            "1234".into()
        );
    }
}
