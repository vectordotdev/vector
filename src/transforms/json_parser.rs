use super::Transform;
use crate::record::Record;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct JsonParserConfig {
    pub field: Option<Atom>,
    pub drop_invalid: bool,
}

impl Default for JsonParserConfig {
    fn default() -> Self {
        Self {
            field: None,
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
    fn transform(&self, mut record: Record) -> Option<Record> {
        let to_parse = self
            .config
            .field
            .as_ref()
            .map_or(Some(record.raw.as_ref()), |field| {
                record.structured.get(field).map(|s| s.as_bytes())
            });

        let parsed = to_parse
            .and_then(|to_parse| serde_json::from_slice::<Value>(to_parse).ok())
            .and_then(|value| {
                if let Value::Object(object) = value {
                    Some(object)
                } else {
                    None
                }
            });

        if let Some(object) = parsed {
            for (name, value) in object {
                insert(&mut record.structured, name, value);
            }
        } else {
            if self.config.drop_invalid {
                return None;
            }
        }

        Some(record)
    }
}

fn insert(structured: &mut HashMap<Atom, String>, name: String, value: Value) {
    match value {
        Value::String(string) => {
            structured.insert(name.into(), string);
        }
        Value::Number(number) => {
            structured.insert(name.into(), number.to_string());
        }
        Value::Bool(b) => {
            structured.insert(name.into(), b.to_string());
        }
        Value::Null => {
            structured.insert(name.into(), "".to_string());
        }
        Value::Array(array) => {
            for (i, element) in array.into_iter().enumerate() {
                let element_name = format!("{}[{}]", name, i);
                insert(structured, element_name, element);
            }
        }
        Value::Object(object) => {
            for (key, value) in object.into_iter() {
                let item_name = format!("{}.{}", name, key);
                insert(structured, item_name, value);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{JsonParser, JsonParserConfig};
    use crate::record::Record;
    use crate::transforms::Transform;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn json_parser_parse_raw() {
        let parser = JsonParser::from(JsonParserConfig {
            ..Default::default()
        });

        let record = Record::from(r#"{"greeting": "hello", "name": "bob"}"#);

        let record = parser.transform(record).unwrap();

        assert_eq!(record.structured[&Atom::from("greeting")], "hello");
        assert_eq!(record.structured[&Atom::from("name")], "bob");
        assert_eq!(record.raw, r#"{"greeting": "hello", "name": "bob"}"#);
    }

    #[test]
    fn json_parser_parse_field() {
        let parser = JsonParser::from(JsonParserConfig {
            field: Some("data".into()),
            ..Default::default()
        });

        // Field present

        let mut record = Record::from("message");
        record.structured.insert(
            "data".into(),
            r#"{"greeting": "hello", "name": "bob"}"#.to_string(),
        );

        let record = parser.transform(record).unwrap();

        assert_eq!(record.structured[&Atom::from("greeting")], "hello");
        assert_eq!(record.structured[&Atom::from("name")], "bob");
        assert_eq!(
            record.structured[&Atom::from("data")],
            r#"{"greeting": "hello", "name": "bob"}"#.to_string()
        );

        // Field missing
        let record = Record::from("message");

        let record = parser.transform(record).unwrap();

        assert!(record.structured.is_empty());
    }

    #[test]
    fn json_parser_invalid_json() {
        let invalid = r#"{"greeting": "hello","#;

        // Raw
        let parser = JsonParser::from(JsonParserConfig {
            field: None,
            ..Default::default()
        });

        let record = Record::from(invalid);

        let record = parser.transform(record).unwrap();

        assert!(record.structured.is_empty());
        assert_eq!(record.raw, invalid);

        // Field
        let parser = JsonParser::from(JsonParserConfig {
            field: Some("data".into()),
            ..Default::default()
        });

        let mut record = Record::from("message");
        record.structured.insert("data".into(), invalid.to_string());

        let record = parser.transform(record).unwrap();

        assert_eq!(record.structured[&Atom::from("data")], invalid);
        assert!(!record.structured.contains_key(&Atom::from("greeting")));
    }

    #[test]
    fn json_parser_drop_invalid() {
        let valid = r#"{"greeting": "hello", "name": "bob"}"#;
        let invalid = r#"{"greeting": "hello","#;
        let not_object = r#""hello""#;

        // Raw
        let parser = JsonParser::from(JsonParserConfig {
            field: None,
            drop_invalid: true,
            ..Default::default()
        });

        let record = Record::from(valid);
        assert!(parser.transform(record).is_some());

        let record = Record::from(invalid);
        assert!(parser.transform(record).is_none());

        let record = Record::from(not_object);
        assert!(parser.transform(record).is_none());

        // Field
        let parser = JsonParser::from(JsonParserConfig {
            field: Some("data".into()),
            drop_invalid: true,
            ..Default::default()
        });

        let mut record = Record::from("message");
        record.structured.insert("data".into(), valid.to_string());
        assert!(parser.transform(record).is_some());

        let mut record = Record::from("message");
        record.structured.insert("data".into(), invalid.to_string());
        assert!(parser.transform(record).is_none());

        let mut record = Record::from("message");
        record
            .structured
            .insert("data".into(), not_object.to_string());
        assert!(parser.transform(record).is_none());

        // Missing field
        let record = Record::from("message");
        assert!(parser.transform(record).is_none());
    }

    #[test]
    fn json_parser_chained() {
        let parser1 = JsonParser::from(JsonParserConfig {
            ..Default::default()
        });
        let parser2 = JsonParser::from(JsonParserConfig {
            field: Some("nested".into()),
            ..Default::default()
        });

        let record = Record::from(r#"{"greeting": "hello", "name": "bob", "nested": "{\"message\": \"help i'm trapped under many layers of json\"}"}"#);
        let record = parser1.transform(record).unwrap();
        let record = parser2.transform(record).unwrap();

        assert_eq!(record.structured[&Atom::from("greeting")], "hello");
        assert_eq!(record.structured[&Atom::from("name")], "bob");
        assert_eq!(
            record.structured[&Atom::from("message")],
            "help i'm trapped under many layers of json"
        );
    }

    #[test]
    fn json_parser_types() {
        let parser = JsonParser::from(JsonParserConfig {
            ..Default::default()
        });

        let record = Record::from(
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
        let record = parser.transform(record).unwrap();

        assert_eq!(record.structured[&Atom::from("string")], "this is text");
        assert_eq!(record.structured[&Atom::from("null")], "");
        assert_eq!(record.structured[&Atom::from("float")], "12.34");
        assert_eq!(record.structured[&Atom::from("int")], "56");
        assert_eq!(record.structured[&Atom::from("bool true")], "true");
        assert_eq!(record.structured[&Atom::from("bool false")], "false");
        assert_eq!(record.structured[&Atom::from("array[0]")], "z");
        assert_eq!(record.structured[&Atom::from("array[1]")], "7");
        assert_eq!(record.structured[&Atom::from("object.nested")], "data");
        assert_eq!(record.structured[&Atom::from("object.more")], "values");
        assert_eq!(
            record.structured[&Atom::from("deep[0][0][0].a.b.c[0][0][0]")],
            "1234"
        );
    }
}
