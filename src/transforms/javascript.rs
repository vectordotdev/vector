use super::Transform;
use crate::{
    event::{Event, ValueKind},
    topology::config::{DataType, TransformConfig},
};
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use lazy_static::lazy_static;
use quick_js::Context;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashSet;
use std::fs;

type JsonObject = serde_json::map::Map<String, JsonValue>;

lazy_static! {
    // although JavaScript identifiers can also contain Unicode characters, we don't allow them
    static ref JS_VALID_IDENTIFIER: Regex = Regex::new(r"^[a-zA-Z_$][a-zA-Z0-9_$]*$").unwrap();
    static ref JS_RESERVED_KEYWORDS: HashSet<&'static str> = [
        // reserved keywords
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "export",
        "extends",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "new",
        "return",
        "super",
        "switch",
        "this",
        "throw",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
        // future reserved keyword
        "enum",
        // literals that cannot be used as identifiers
        "null",
        "true",
        "false"
    ]
    .iter()
    .cloned()
    .collect();
    static ref JS_RUNTIME_LIBRARY: &'static str = r#"
        function __vector_json_reviver(key, value) {
            if (value && value.hasOwnProperty('$date')) {
                return new Date(value.$date)
            }
            return value
        }

        function __vector_json_replacer(key, value) {
            if (this[key] instanceof Date) {
                return {
                    $date: this[key].toISOString()
                }
            }
            return value
        }

        const __vector_decode = data => JSON.parse(data,  __vector_json_reviver)
        const __vector_encode = data => JSON.stringify(data , __vector_json_replacer)
        "#;
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct JavaScriptConfig {
    source: Option<String>,
    path: Option<String>,
    handler: Option<String>,
    memory_limit: Option<usize>,
}

#[typetag::serde(name = "javascript")]
impl TransformConfig for JavaScriptConfig {
    fn build(&self) -> Result<Box<dyn Transform>, String> {
        JavaScript::new(
            self.source.clone(),
            self.path.clone(),
            self.handler.clone(),
            self.memory_limit.clone(),
        )
        .map(|js| -> Box<dyn Transform> { Box::new(js) })
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

pub struct JavaScript {
    ctx: Context,
    handler: String,
}

// See https://www.freelists.org/post/quickjs-devel/Usage-of-QuickJS-in-multithreaded-environments,1
unsafe impl Send for JavaScript {}

impl JavaScript {
    pub fn new(
        source: Option<String>,
        path: Option<String>,
        handler: Option<String>,
        memory_limit: Option<usize>,
    ) -> Result<Self, String> {
        // validate and load source
        let source = match (source, path) {
            (Some(source), None) => source,
            (None, Some(path)) => fs::read_to_string(&path).map_err(|err| {
                format!("Cannot load JavaScript source from \"{}\": {}", path, err)
            })?,
            (Some(_), Some(_)) => {
                return Err("\"source\" and \"path\" cannot be provided together".to_string())
            }
            (None, None) => {
                return Err("Either \"source\" or \"path\" should be provided".to_string())
            }
        };

        // validate handler parameter if present
        if let Some(ref handler) = handler {
            if !JS_VALID_IDENTIFIER.is_match(&handler) {
                return Err(format!(
                    "Handler name \"{}\" is not a valid JavaScript identifier",
                    handler
                ));
            }
            if JS_RESERVED_KEYWORDS.contains(&handler[..]) {
                return Err(format!(
                    "Handler name \"{}\" is reserved in JavaScript and cannot be used",
                    handler
                ));
            }
        }

        // init QuickJS context
        let mut builder = Context::builder();
        if let Some(memory_limit) = memory_limit {
            builder = builder.memory_limit(memory_limit);
        }
        let ctx = builder
            .build()
            .map_err(|err| format!("Cannot create JavaScript runtime: {}", err))?;
        ctx.eval(&JS_RUNTIME_LIBRARY)
            .map_err(|err| format!("Cannot load JavaScript runtime library: {}", err))?;

        // inject handler
        let (handler, source) = if let Some(handler) = handler {
            (handler, format!(r"{}; null", source))
        } else {
            let handler = "__vector_handler".to_string();
            let source = format!(r"const {} = ({})", handler, source);
            (handler, source)
        };
        ctx.eval(&source)
            .map_err(|err| format!("Cannot create handler: {}", err))?;

        // check that handler is a function
        let handler_is_ok: bool = ctx
            .eval_as(&format!(r"typeof {} === 'function'", handler))
            .map_err(|err| format!("Cannot validate handler: {}", err))?;

        if handler_is_ok {
            Ok(Self { ctx, handler })
        } else {
            Err("Handler is not a function".to_string())
        }
    }

    pub fn process(&self, output: &mut Vec<Event>, event: Event) -> Result<(), String> {
        let encoded = encode(event)?;
        let transformed: String = self
            .ctx
            .eval_as(&format!(
                r"__vector_encode({}(__vector_decode({})))",
                self.handler,
                serde_json::to_string(&encoded).unwrap(),
            ))
            .map_err(|err| format!("Runtime error in JavaScript code: {}", err))?;
        decode_and_write(&transformed, output)?;
        Ok(())
    }
}

fn encode(event: Event) -> Result<String, String> {
    let mut json_event = serde_json::map::Map::new();
    for (key, value) in event.as_log().all_fields() {
        let value = match value {
            // encode dates
            ValueKind::Timestamp(timestamp) => json!({
                "$date": timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
            }),
            // encode other types of fields
            _ => serde_json::to_value(value)
                .map_err(|err| format!("Cannot serialize field \"{}\": {}", key, err))?,
        };
        json_event.insert(key.to_string(), value);
    }
    Ok(serde_json::to_string(&JsonValue::Object(json_event)).unwrap())
}

fn decode_and_write(json: &str, output: &mut Vec<Event>) -> Result<(), String> {
    let value = serde_json::from_str(json)
        .map_err(|err| format!("Cannot parse JSON returned from JavaScript: {}", err))?;
    write_value(value, output)
}

fn write_value(value: JsonValue, output: &mut Vec<Event>) -> Result<(), String> {
    match value {
        JsonValue::Array(json_events) => {
            for json_event in json_events {
                write_event(json_event, output)?;
            }
            Ok(())
        }
        _ => write_event(value, output),
    }
}

fn write_event(json_event: JsonValue, output: &mut Vec<Event>) -> Result<(), String> {
    match json_event {
        JsonValue::Null => Ok(()),
        JsonValue::Object(object) => {
            let event = object_to_event(object)?;
            output.push(event);
            Ok(())
        }
        _ => Err(format!(
            "Expected event object or null, found: {}",
            serde_json::to_string(&json_event).unwrap()
        )),
    }
}

fn object_to_event(object: JsonObject) -> Result<Event, String> {
    let mut event = Event::new_empty_log();
    let log = event.as_mut_log();
    for (k, v) in object.into_iter() {
        let v = match v {
            JsonValue::Null => continue,
            JsonValue::Bool(v) => ValueKind::Boolean(v),
            JsonValue::Number(v) => {
                // NB: maybe use BigInt as integer type in JavaScript?
                if v.is_i64() {
                    ValueKind::Integer(v.as_i64().unwrap())
                } else {
                    ValueKind::Float(v.as_f64().unwrap())
                }
            }
            JsonValue::String(v) => ValueKind::Bytes(v.into()),
            JsonValue::Object(v) => {
                if let Some(JsonValue::String(date)) = v.get("$date") {
                    DateTime::parse_from_rfc3339(&date)
                        .map(|timestamp| Utc.from_utc_datetime(&timestamp.naive_utc()))
                        .map(ValueKind::Timestamp)
                        .map_err(|err| format!("Unable to deserialize date: {}", err))?
                } else {
                    return Err(format!(
                        "Nested objects inside events are not supported, \
                         but field \"{}\" contains an object: {}",
                        k,
                        serde_json::to_string(&v).unwrap()
                    ));
                }
            }
            JsonValue::Array(v) => {
                return Err(format!(
                    "Arrays inside events are not supported, \
                     but field \"{}\" contains an array: {}",
                    k,
                    serde_json::to_string(&v).unwrap()
                ))
            }
        };
        log.insert_implicit(k.into(), v.into());
    }
    Ok(event)
}

impl Transform for JavaScript {
    // only used in tests
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut output = Vec::new();
        self.transform_into(&mut output, event);
        assert!(output.len() <= 1);
        output.pop()
    }

    fn transform_into(&mut self, output: &mut Vec<Event>, event: Event) {
        self.process(output, event).unwrap_or_else(|err| {
            error!("Error in JavaScript transform; discarding event\n{}", err)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::JavaScript;
    use crate::{
        event::{Event, ValueKind, TIMESTAMP},
        transforms::Transform,
    };
    use chrono::{TimeZone, Utc};
    use mktemp::Temp;
    use std::fs;

    #[test]
    fn javascript_new() {
        let res = JavaScript::new(Some(r"event => event".to_string()), None, None, None);
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_path() {
        let path = Temp::new_path();
        let path_str = path.to_str().unwrap();
        fs::write(path_str, r"event => event").unwrap();
        let res = JavaScript::new(None, Some(path_str.to_string()), None, None);
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_source_and_path() {
        let source = r"event => event";

        let path = Temp::new_path();
        let path_str = path.to_str().unwrap();
        fs::write(path_str, source).unwrap();
        let res = JavaScript::new(
            Some(source.to_string()),
            Some(path_str.to_string()),
            None,
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_without_source_and_path() {
        let res = JavaScript::new(None, None, None, None);
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_syntax_error() {
        let res = JavaScript::new(Some(r"...".to_string()), None, None, None);
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_not_a_function() {
        let res = JavaScript::new(Some(r"123".to_string()), None, None, None);
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_with_handler_function() {
        let res = JavaScript::new(
            Some(
                r#"
                function handler(event) {
                    return event
                }
                "#
                .to_string(),
            ),
            None,
            Some("handler".to_string()),
            None,
        );
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_handler_const() {
        let res = JavaScript::new(
            Some(
                r#"
                const handler = event => event
                "#
                .to_string(),
            ),
            None,
            Some("handler".to_string()),
            None,
        );
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_handler_global_object_property() {
        let res = JavaScript::new(
            Some(
                r#"
                handler = event => event
                "#
                .to_string(),
            ),
            None,
            Some("handler".to_string()),
            None,
        );
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_handler_missing() {
        let res = JavaScript::new(
            Some(
                r#"
                const handler1 = event => event
                "#
                .to_string(),
            ),
            None,
            Some("handler2".to_string()),
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_with_handler_not_a_valid_identifier() {
        let res = JavaScript::new(
            Some(r"event => event".to_string()),
            None,
            Some("!@#$".to_string()),
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_with_handler_reserved_keyword() {
        let res = JavaScript::new(
            Some(r"event => event".to_string()),
            None,
            Some("new".to_string()),
            None,
        );
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_with_memory_limit_success() {
        let res = JavaScript::new(
            Some(r"event => event".to_string()),
            None,
            None,
            Some(10_000_000),
        );
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_memory_limit_failure_oom() {
        let res = JavaScript::new(Some(r"event => event".to_string()), None, None, Some(10));
        assert!(res.is_err());
    }

    fn make_js(source: &str) -> JavaScript {
        JavaScript::new(Some(source.to_string()), None, None, None).unwrap()
    }

    fn make_js_with_handler(source: &str, handler: &str) -> JavaScript {
        JavaScript::new(
            Some(source.to_string()),
            None,
            Some(handler.to_string()),
            None,
        )
        .unwrap()
    }

    fn make_event() -> Event {
        let mut event = Event::from("some text");
        let log = event.as_mut_log();
        // only millisecond precision is supported by JavaScript,
        // so our timestamp has millisecond precision in order to
        // not lose precision after conversions
        log.insert_implicit(
            TIMESTAMP.clone(),
            ValueKind::Timestamp(Utc.ymd(2020, 1, 2).and_hms_milli(3, 4, 5, 6)),
        );
        event
    }
    #[test]
    fn javascript_transform_discard_event() {
        let mut js = make_js(
            r#"
            event => null
            "#,
        );
        assert!(js.transform(make_event()).is_none());
    }

    #[test]
    fn javascript_transform_identity() {
        let mut js = make_js(
            r#"
            event => event
            "#,
        );

        let source_event = make_event();
        let transformed_event = js.transform(source_event.clone());
        assert_eq!(transformed_event, Some(source_event));
    }

    #[test]
    fn javascript_transform_add_field_string() {
        let mut js = make_js(
            r#"
            event => ({...event, field: 'value'})
            "#,
        );

        let source_event = make_event();

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.insert_implicit("field".into(), "value".into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));
    }

    #[test]
    fn javascript_transform_add_field_integer() {
        let mut js = make_js(
            r#"
            event => ({...event, field: 123})
            "#,
        );

        let source_event = make_event();

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.insert_implicit("field".into(), 123.into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));
    }

    #[test]
    fn javascript_transform_add_field_float() {
        let mut js = make_js(
            r#"
            event => ({...event, field: 3.14159})
            "#,
        );

        let source_event = make_event();

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.insert_implicit("field".into(), 3.14159.into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));
    }

    #[test]
    fn javascript_transform_add_field_bool() {
        let mut js = make_js(
            r#"
            event => ({...event, field: true})
            "#,
        );

        let source_event = make_event();

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.insert_implicit("field".into(), true.into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));
    }

    #[test]
    fn javascript_transform_add_field_date() {
        let mut js = make_js(
            r#"
            event => ({...event, field: new Date('2020-01-01T00:00:00.123Z')})
            "#,
        );

        let source_event = make_event();

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        let date = Utc.ymd(2020, 1, 1).and_hms_milli(0, 0, 0, 123);
        expected_log.insert_implicit("field".into(), date.into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));
    }

    #[test]
    fn javascript_transform_no_nested_objects() {
        let mut js = make_js(
            r#"
                event => ({...event, field: {a: 3, b: 4}})
                "#,
        );

        let source_event = make_event();
        let transformed_event = js.transform(source_event);
        assert!(transformed_event.is_none());
    }

    #[test]
    fn javascript_transform_no_nested_arrays() {
        let mut js = make_js(
            r#"
            event => ({...event, field: [1,2,3]})
            "#,
        );

        let source_event = make_event();
        let transformed_event = js.transform(source_event);
        assert!(transformed_event.is_none());
    }

    #[test]
    fn javascript_transform_remove_field() {
        let mut js = make_js(
            r#"
            event => {
                delete event.field
                return event
            }
            "#,
        );

        let mut source_event = make_event();
        let source_log = source_event.as_mut_log();
        source_log.insert_implicit("field".into(), "value".into());

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.remove(&"field".into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));
    }

    #[test]
    #[ignore] // See https://www.freelists.org/post/quickjs-devel/Bug-report-JSONstringify-produces-invalid-JSON-with-replacer-that-returns-undefined
    fn javascript_transform_remove_field_set_undefined() {
        let mut js = make_js(
            r#"
            event => ({...event, field: undefined})
            "#,
        );

        let mut source_event = make_event();
        let source_log = source_event.as_mut_log();
        source_log.insert_implicit("field".into(), "value".into());

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.remove(&"field".into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));
    }

    #[test]
    fn javascript_transform_remove_field_set_null() {
        let mut js = make_js(
            r#"
            event => ({...event, field: null})
            "#,
        );

        let mut source_event = make_event();
        let source_log = source_event.as_mut_log();
        source_log.insert_implicit("field".into(), "value".into());

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.remove(&"field".into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));
    }

    #[test]
    fn javascript_transform_output_multiple_events() {
        let mut js = make_js(
            r#"
            event => [{...event, a: 3}, {...event, b: 4}]
            "#,
        );

        let source_event = make_event();
        let mut expected_events = vec![];

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.insert_implicit("a".into(), 3.into());
        expected_events.push(expected_event);

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.insert_implicit("b".into(), 4.into());
        expected_events.push(expected_event);

        let mut transformed_events = vec![];
        js.transform_into(&mut transformed_events, source_event);
        assert_eq!(transformed_events, expected_events);
    }

    #[test]
    fn javascript_transform_with_state() {
        let mut js = make_js_with_handler(
            r#"
            let count = 0
            const handler = event => ({...event, count: ++count})
            "#,
            "handler",
        );

        // first event
        let source_event = make_event();

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.insert_implicit("count".into(), 1.into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));

        // second event
        let source_event = make_event();

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.insert_implicit("count".into(), 2.into());

        let transformed_event = js.transform(source_event);
        assert_eq!(transformed_event, Some(expected_event));
    }
}
