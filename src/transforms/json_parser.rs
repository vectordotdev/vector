use super::Transform;
use crate::{
    event::{self, Event, ValueKind},
    topology::config::{DataType, TransformConfig},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct JsonParserConfig {
    pub field: Option<Atom>,
    pub drop_invalid: bool,
    #[derivative(Default(value = "true"))]
    pub drop_field: bool,
}

#[typetag::serde(name = "json_parser")]
impl TransformConfig for JsonParserConfig {
    fn build(&self) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(JsonParser::from(self.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

pub struct JsonParser {
    field: Atom,
    drop_invalid: bool,
    drop_field: bool,
}

impl From<JsonParserConfig> for JsonParser {
    fn from(config: JsonParserConfig) -> JsonParser {
        let field = if let Some(field) = &config.field {
            field
        } else {
            &event::MESSAGE
        };

        JsonParser {
            field: field.clone(),
            drop_invalid: config.drop_invalid,
            drop_field: config.drop_field,
        }
    }
}

impl Transform for JsonParser {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let to_parse = event.as_log().get(&self.field).map(|s| s.as_bytes());

        let parsed = to_parse
            .and_then(|to_parse| {
                serde_json::from_slice::<Value>(to_parse.as_ref())
                    .map_err(|error| {
                        debug!(
                            message = "Event failed to parse as JSON",
                            field = self.field.as_ref(),
                            %error,
                            rate_limit_secs = 30
                        )
                    })
                    .ok()
            })
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
        } else if self.drop_invalid {
            return None;
        }

        if self.drop_field {
            event.as_mut_log().remove(&self.field);
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
            let val = if let Some(val) = number.as_i64() {
                ValueKind::from(val)
            } else if let Some(val) = number.as_f64() {
                ValueKind::from(val)
            } else {
                ValueKind::from(number.to_string())
            };

            event.as_mut_log().insert_explicit(name.into(), val);
        }
        Value::Bool(b) => {
            event.as_mut_log().insert_explicit(name.into(), b.into());
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
    fn json_parser_drop_field() {
        let mut parser = JsonParser::from(JsonParserConfig::default());

        let event = Event::from(r#"{"greeting": "hello", "name": "bob"}"#);

        let event = parser.transform(event).unwrap();

        assert!(event.as_log().get(&event::MESSAGE).is_none());
    }

    #[test]
    fn json_parser_doesnt_drop_field() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: false,
            ..Default::default()
        });

        let event = Event::from(r#"{"greeting": "hello", "name": "bob"}"#);

        let event = parser.transform(event).unwrap();

        assert!(event.as_log().get(&event::MESSAGE).is_some());
    }

    #[test]
    fn json_parser_parse_raw() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: false,
            ..Default::default()
        });

        let event = Event::from(r#"{"greeting": "hello", "name": "bob"}"#);

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log()[&Atom::from("greeting")], "hello".into());
        assert_eq!(event.as_log()[&Atom::from("name")], "bob".into());
        assert_eq!(
            event.as_log()[&event::MESSAGE],
            r#"{"greeting": "hello", "name": "bob"}"#.into()
        );
    }

    #[test]
    fn json_parser_parse_field() {
        let mut parser = JsonParser::from(JsonParserConfig {
            field: Some("data".into()),
            drop_field: false,
            ..Default::default()
        });

        // Field present

        let mut event = Event::from("message");
        event.as_mut_log().insert_explicit(
            "data".into(),
            r#"{"greeting": "hello", "name": "bob"}"#.into(),
        );

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log()[&Atom::from("greeting")], "hello".into(),);
        assert_eq!(event.as_log()[&Atom::from("name")], "bob".into());
        assert_eq!(
            event.as_log()[&Atom::from("data")],
            r#"{"greeting": "hello", "name": "bob"}"#.into()
        );

        // Field missing
        let event = Event::from("message");

        let parsed = parser.transform(event.clone()).unwrap();

        assert_eq!(event, parsed);
    }

    #[test]
    fn json_parser_parse_inner_json() {
        let mut parser_outter = JsonParser::from(JsonParserConfig {
            ..Default::default()
        });

        let mut parser_inner = JsonParser::from(JsonParserConfig {
            field: Some("log".into()),
            ..Default::default()
        });

        let event = Event::from(r#"{"log":"{\"type\":\"response\",\"@timestamp\":\"2018-10-04T21:12:33Z\",\"tags\":[],\"pid\":1,\"method\":\"post\",\"statusCode\":200,\"req\":{\"url\":\"/elasticsearch/_msearch\",\"method\":\"post\",\"headers\":{\"host\":\"logs.com\",\"connection\":\"close\",\"x-real-ip\":\"120.21.3.1\",\"x-forwarded-for\":\"121.91.2.2\",\"x-forwarded-host\":\"logs.com\",\"x-forwarded-port\":\"443\",\"x-forwarded-proto\":\"https\",\"x-original-uri\":\"/elasticsearch/_msearch\",\"x-scheme\":\"https\",\"content-length\":\"1026\",\"accept\":\"application/json, text/plain, */*\",\"origin\":\"https://logs.com\",\"kbn-version\":\"5.2.3\",\"user-agent\":\"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_12_6) AppleWebKit/532.30 (KHTML, like Gecko) Chrome/62.0.3361.210 Safari/533.21\",\"content-type\":\"application/x-ndjson\",\"referer\":\"https://domain.com/app/kibana\",\"accept-encoding\":\"gzip, deflate, br\",\"accept-language\":\"en-US,en;q=0.8\"},\"remoteAddress\":\"122.211.22.11\",\"userAgent\":\"22.322.32.22\",\"referer\":\"https://domain.com/app/kibana\"},\"res\":{\"statusCode\":200,\"responseTime\":417,\"contentLength\":9},\"message\":\"POST /elasticsearch/_msearch 200 225ms - 8.0B\"}\n","stream":"stdout","time":"2018-10-02T21:14:48.2233245241Z"}"#);

        let parsed_event = parser_outter.transform(event).unwrap();

        assert_eq!(
            parsed_event.as_log()[&Atom::from("stream")],
            "stdout".into()
        );

        let parsed_inner_event = parser_inner.transform(parsed_event).unwrap();
        let log = parsed_inner_event.into_log();

        assert_eq!(log[&Atom::from("type")], "response".into());
        assert_eq!(log[&Atom::from("statusCode")], 200.into());
    }

    #[test]
    fn json_parser_invalid_json() {
        let invalid = r#"{"greeting": "hello","#;

        // Raw
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: false,
            ..Default::default()
        });

        let event = Event::from(invalid);

        let parsed = parser.transform(event.clone()).unwrap();

        assert_eq!(event, parsed);
        assert_eq!(event.as_log()[&event::MESSAGE], invalid.into());

        // Field
        let mut parser = JsonParser::from(JsonParserConfig {
            field: Some("data".into()),
            drop_field: false,
            ..Default::default()
        });

        let mut event = Event::from("message");
        event
            .as_mut_log()
            .insert_explicit("data".into(), invalid.into());

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log()[&Atom::from("data")], invalid.into());
        assert!(event.as_log().get(&Atom::from("greeting")).is_none());
    }

    #[test]
    fn json_parser_drop_invalid() {
        let valid = r#"{"greeting": "hello", "name": "bob"}"#;
        let invalid = r#"{"greeting": "hello","#;
        let not_object = r#""hello""#;

        // Raw
        let mut parser = JsonParser::from(JsonParserConfig {
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
        let mut parser = JsonParser::from(JsonParserConfig {
            field: Some("data".into()),
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
        let mut parser1 = JsonParser::from(JsonParserConfig {
            ..Default::default()
        });
        let mut parser2 = JsonParser::from(JsonParserConfig {
            field: Some("nested".into()),
            ..Default::default()
        });

        let event = Event::from(r#"{"greeting": "hello", "name": "bob", "nested": "{\"message\": \"help i'm trapped under many layers of json\"}"}"#);
        let event = parser1.transform(event).unwrap();
        let event = parser2.transform(event).unwrap();

        assert_eq!(event.as_log()[&Atom::from("greeting")], "hello".into());
        assert_eq!(event.as_log()[&Atom::from("name")], "bob".into());
        assert_eq!(
            event.as_log()[&Atom::from("message")],
            "help i'm trapped under many layers of json".into()
        );
    }

    #[test]
    fn json_parser_types() {
        let mut parser = JsonParser::from(JsonParserConfig {
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

        assert_eq!(event.as_log()[&Atom::from("string")], "this is text".into());
        assert_eq!(event.as_log()[&Atom::from("null")], "".into());
        assert_eq!(event.as_log()[&Atom::from("float")], 12.34.into());
        assert_eq!(event.as_log()[&Atom::from("int")], 56.into());
        assert_eq!(event.as_log()[&Atom::from("bool true")], true.into());
        assert_eq!(event.as_log()[&Atom::from("bool false")], false.into());
        assert_eq!(event.as_log()[&Atom::from("array[0]")], "z".into());
        assert_eq!(event.as_log()[&Atom::from("array[1]")], 7.into());
        assert_eq!(event.as_log()[&Atom::from("object.nested")], "data".into());
        assert_eq!(event.as_log()[&Atom::from("object.more")], "values".into());
        assert_eq!(
            event.as_log()[&Atom::from("deep[0][0][0].a.b.c[0][0][0]")],
            1234.into()
        );
    }
}
