use super::Transform;
use crate::{
    event::{self, Event},
    internal_events::{JsonEventProcessed, JsonFailedParse},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
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
    pub target_field: Option<String>,
    pub overwrite_target: Option<bool>,
}

inventory::submit! {
    TransformDescription::new::<JsonParserConfig>("json_parser")
}

#[typetag::serde(name = "json_parser")]
impl TransformConfig for JsonParserConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(JsonParser::from(self.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "json_parser"
    }
}

#[derive(Debug)]
pub struct JsonParser {
    field: Atom,
    drop_invalid: bool,
    drop_field: bool,
    target_field: Option<Atom>,
    overwrite_target: bool,
}

impl From<JsonParserConfig> for JsonParser {
    fn from(config: JsonParserConfig) -> JsonParser {
        let field = if let Some(field) = &config.field {
            field
        } else {
            &event::log_schema().message_key()
        };

        JsonParser {
            field: field.clone(),
            drop_invalid: config.drop_invalid,
            drop_field: config.drop_field,
            target_field: config.target_field.map(Atom::from),
            overwrite_target: config.overwrite_target.unwrap_or(false),
        }
    }
}

impl Transform for JsonParser {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        let log = event.as_mut_log();
        let to_parse = log.get(&self.field).map(|s| s.as_bytes());

        emit!(JsonEventProcessed);

        let parsed = to_parse
            .and_then(|to_parse| {
                serde_json::from_slice::<Value>(to_parse.as_ref())
                    .map_err(|error| {
                        emit!(JsonFailedParse {
                            field: &self.field,
                            error: error
                        })
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
            match self.target_field {
                Some(ref target_field) => {
                    let contains_target = log.contains(&target_field);

                    if contains_target && !self.overwrite_target {
                        error!(message = "target field already exists", %target_field);
                    } else {
                        if self.drop_field {
                            log.remove(&self.field);
                        }

                        log.insert(&target_field, Value::Object(object));
                    }
                }
                None => {
                    if self.drop_field {
                        log.remove(&self.field);
                    }

                    for (key, value) in object {
                        log.insert_flat(key, value);
                    }
                }
            }
        } else if self.drop_invalid {
            return None;
        }

        Some(event)
    }
}

#[cfg(test)]
mod test {
    use super::{JsonParser, JsonParserConfig};
    use crate::event::{self, Event};
    use crate::transforms::Transform;
    use serde_json::json;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn json_parser_drop_field() {
        let mut parser = JsonParser::from(JsonParserConfig::default());

        let event = Event::from(r#"{"greeting": "hello", "name": "bob"}"#);

        let event = parser.transform(event).unwrap();

        assert!(event
            .as_log()
            .get(&event::log_schema().message_key())
            .is_none());
    }

    #[test]
    fn json_parser_doesnt_drop_field() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: false,
            ..Default::default()
        });

        let event = Event::from(r#"{"greeting": "hello", "name": "bob"}"#);

        let event = parser.transform(event).unwrap();

        assert!(event
            .as_log()
            .get(&event::log_schema().message_key())
            .is_some());
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
            event.as_log()[&event::log_schema().message_key()],
            r#"{"greeting": "hello", "name": "bob"}"#.into()
        );
    }

    // Ensure the JSON parser doesn't take strings as toml paths.
    // This is a regression test, see: https://github.com/timberio/vector/issues/2814
    #[test]
    fn json_parser_parse_periods() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: false,
            ..Default::default()
        });

        let test_json = json!({
            "field.with.dots": "hello",
            "sub.field": { "another.one": "bob"},
        });

        let event = Event::from(test_json.to_string());

        let event = parser.transform(event).unwrap();

        assert_eq!(
            event.as_log().get_flat(&Atom::from("field.with.dots")),
            Some(&crate::event::Value::from("hello")),
        );
        assert_eq!(
            event.as_log().get_flat(&Atom::from("sub.field")),
            Some(&crate::event::Value::from(json!({ "another.one": "bob", }))),
        );
    }

    #[test]
    fn json_parser_parse_raw_with_whitespace() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: false,
            ..Default::default()
        });

        let event = Event::from(r#" {"greeting": "hello", "name": "bob"}    "#);

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log()[&Atom::from("greeting")], "hello".into());
        assert_eq!(event.as_log()[&Atom::from("name")], "bob".into());
        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            r#" {"greeting": "hello", "name": "bob"}    "#.into()
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
        event
            .as_mut_log()
            .insert("data", r#"{"greeting": "hello", "name": "bob"}"#);

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

        let event = Event::from(
            r#"{"log":"{\"type\":\"response\",\"@timestamp\":\"2018-10-04T21:12:33Z\",\"tags\":[],\"pid\":1,\"method\":\"post\",\"statusCode\":200,\"req\":{\"url\":\"/elasticsearch/_msearch\",\"method\":\"post\",\"headers\":{\"host\":\"logs.com\",\"connection\":\"close\",\"x-real-ip\":\"120.21.3.1\",\"x-forwarded-for\":\"121.91.2.2\",\"x-forwarded-host\":\"logs.com\",\"x-forwarded-port\":\"443\",\"x-forwarded-proto\":\"https\",\"x-original-uri\":\"/elasticsearch/_msearch\",\"x-scheme\":\"https\",\"content-length\":\"1026\",\"accept\":\"application/json, text/plain, */*\",\"origin\":\"https://logs.com\",\"kbn-version\":\"5.2.3\",\"user-agent\":\"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_12_6) AppleWebKit/532.30 (KHTML, like Gecko) Chrome/62.0.3361.210 Safari/533.21\",\"content-type\":\"application/x-ndjson\",\"referer\":\"https://domain.com/app/kibana\",\"accept-encoding\":\"gzip, deflate, br\",\"accept-language\":\"en-US,en;q=0.8\"},\"remoteAddress\":\"122.211.22.11\",\"userAgent\":\"22.322.32.22\",\"referer\":\"https://domain.com/app/kibana\"},\"res\":{\"statusCode\":200,\"responseTime\":417,\"contentLength\":9},\"message\":\"POST /elasticsearch/_msearch 200 225ms - 8.0B\"}\n","stream":"stdout","time":"2018-10-02T21:14:48.2233245241Z"}"#,
        );

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
        assert_eq!(
            event.as_log()[&event::log_schema().message_key()],
            invalid.into()
        );

        // Field
        let mut parser = JsonParser::from(JsonParserConfig {
            field: Some("data".into()),
            drop_field: false,
            ..Default::default()
        });

        let mut event = Event::from("message");
        event.as_mut_log().insert("data", invalid);

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
        event.as_mut_log().insert("data", valid);
        assert!(parser.transform(event).is_some());

        let mut event = Event::from("message");
        event.as_mut_log().insert("data", invalid);
        assert!(parser.transform(event).is_none());

        let mut event = Event::from("message");
        event.as_mut_log().insert("data", not_object);
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

        let event = Event::from(
            r#"{"greeting": "hello", "name": "bob", "nested": "{\"message\": \"help i'm trapped under many layers of json\"}"}"#,
        );
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
        assert_eq!(
            event.as_log()[&Atom::from("null")],
            crate::event::Value::Null
        );
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

    #[test]
    fn drop_field_before_adding() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: true,
            ..Default::default()
        });

        let event = Event::from(
            r#"{
                "key": "data",
                "message": "inner"
            }"#,
        );

        let event = parser.transform(event).unwrap();

        assert_eq!(event.as_log()[&Atom::from("key")], "data".into());
        assert_eq!(event.as_log()[&Atom::from("message")], "inner".into());
    }

    #[test]
    fn doesnt_drop_field_after_failed_parse() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: true,
            ..Default::default()
        });

        let event = Event::from(r#"invalid json"#);

        let event = parser.transform(event).unwrap();

        assert_eq!(
            event.as_log()[&Atom::from("message")],
            "invalid json".into()
        );
    }

    #[test]
    fn target_field_works() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: false,
            target_field: Some("that".into()),
            ..Default::default()
        });

        let event = Event::from(r#"{"greeting": "hello", "name": "bob"}"#);
        let event = parser.transform(event).unwrap();
        let event = event.as_log();

        assert_eq!(event[&Atom::from("that.greeting")], "hello".into());
        assert_eq!(event[&Atom::from("that.name")], "bob".into());
    }

    #[test]
    fn target_field_preserves_existing() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: false,
            target_field: Some("message".into()),
            ..Default::default()
        });

        let message = r#"{"greeting": "hello", "name": "bob"}"#;
        let event = Event::from(message);
        let event = parser.transform(event).unwrap();
        let event = event.as_log();

        assert_eq!(event[&"message".into()], message.into());
        assert_eq!(event.get(&"message.greeting".into()), None);
        assert_eq!(event.get(&"message.name".into()), None);
    }

    #[test]
    fn target_field_overwrites_existing() {
        let mut parser = JsonParser::from(JsonParserConfig {
            drop_field: false,
            target_field: Some("message".into()),
            overwrite_target: Some(true),
            ..Default::default()
        });

        let message = r#"{"greeting": "hello", "name": "bob"}"#;
        let event = Event::from(message);
        let event = parser.transform(event).unwrap();
        let event = event.as_log();

        match event.get(&"message".into()) {
            Some(crate::event::Value::Map(_)) => (),
            _ => panic!("\"message\" is not a map"),
        }
        assert_eq!(event[&Atom::from("message.greeting")], "hello".into());
        assert_eq!(event[&Atom::from("message.name")], "bob".into());
    }
}
