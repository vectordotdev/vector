use super::Transform;
use crate::{
    event::{Event, ValueKind},
    topology::config::{DataType, TransformConfig},
};
use lazy_static::lazy_static;
use quick_js::{Context, JsValue};
use regex::Regex;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::{
    collections::{HashMap, HashSet},
    fs,
    sync::mpsc,
    thread,
};

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
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Cannot load JavaScript source from \"{}\": {}", path, source))]
    JavascriptLoadSourceError {
        path: String,
        source: std::io::Error,
    },
    #[snafu(display("Handler name \"{}\" is not a valid JavaScript identifier", handler))]
    JavascriptHandlerIsNotIdentifierError { handler: String },
    #[snafu(display(
        "Handler name \"{}\" is reserved in JavaScript and cannot be used",
        handler
    ))]
    JavascriptHandlerReservedError { handler: String },
    #[snafu(display("Cannot create JavaScript runtime: {}", source))]
    JavascriptRuntimeCreationError { source: quick_js::ContextError },
    #[snafu(display("Cannot inject handler: {}", source))]
    JavascriptInjectionError { source: quick_js::ExecutionError },
    #[snafu(display("Cannot validate handler: {}", source))]
    JavascriptValidationError { source: quick_js::ExecutionError },
    #[snafu(display("Handler is not a function"))]
    JavascriptHandlerIsNotAFunctionError,
}
#[derive(Debug, Snafu)]
enum ProcessError {
    #[snafu(display("Expected event object or null, found: {:?}", js_event))]
    JavascriptUnexpectedValueError { js_event: JsValue },
    #[snafu(display("Runtime error in JavaScript code: {}", source))]
    JavascriptRuntimeError { source: quick_js::ExecutionError },
    #[snafu(display(
        "Nested objects inside events are not supported, but field \"{}\" contains an object: {:?}",
        field,
        value
    ))]
    JavascriptNestedObjectsError {
        field: String,
        value: HashMap<String, JsValue>,
    },
    #[snafu(display(
        "Arrays inside events are not supported, but field \"{}\" contains an array: {:?}",
        field,
        value
    ))]
    JavascriptNestedArraysError { field: String, value: Vec<JsValue> },
    #[snafu(display(
        "BigInts returned from JavaScript should fit into 64-bit signed integers, got {}",
        value
    ))]
    JavascriptBigintOverflowError { value: quick_js::BigInt },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged, deny_unknown_fields)]
pub enum SourceOrPath {
    Source { source: String },
    Path { path: String },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct JavaScriptConfig {
    #[serde(flatten)]
    pub source_or_path: SourceOrPath,
    pub handler: Option<String>,
    pub memory_limit: Option<usize>,
}

#[typetag::serde(name = "javascript")]
impl TransformConfig for JavaScriptConfig {
    fn build(&self) -> crate::Result<Box<dyn Transform>> {
        JavaScript::new(self.clone()).map(|js| -> Box<dyn Transform> { Box::new(js) })
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

enum ProcessorInput {
    Event(Event),
    Stop,
}

enum ProcessorOutput {
    Start(crate::Result<()>),
    Events(Vec<Event>),
}

pub struct JavaScript {
    input: mpsc::SyncSender<ProcessorInput>,
    output: mpsc::Receiver<ProcessorOutput>,
}

impl JavaScript {
    pub fn new(config: JavaScriptConfig) -> crate::Result<Self> {
        let (input, thread_input) = mpsc::sync_channel(0);
        let (thread_output, output) = mpsc::sync_channel(0);

        let dispatcher = tracing::dispatcher::get_default(|d| d.clone());
        thread::spawn(move || {
            let processor = JavaScriptProcessor::new(config);
            let processor = match processor {
                Ok(processor) => {
                    thread_output
                        .send(ProcessorOutput::Start(Ok(())))
                        .expect("Unable to send data from JavaScript thread");
                    processor
                }
                Err(e) => {
                    return thread_output
                        .send(ProcessorOutput::Start(Err(e)))
                        .expect("Unable to send data from JavaScript thread");
                }
            };

            let dispatcher = dispatcher;
            tracing::dispatcher::with_default(&dispatcher, || {
                for event in thread_input {
                    match event {
                        ProcessorInput::Event(event) => {
                            let mut output_events = Vec::new();
                            let result = processor.process(&mut output_events, event);
                            if let Err(error) = result {
                                warn!(message = "Error in JavaScript transform; discarding event", error = ?error)
                            }
                            thread_output
                                .send(ProcessorOutput::Events(output_events))
                                .expect("Unable to send data from JavaScript thread");
                        }
                        ProcessorInput::Stop => break,
                    }
                }
            });
        });
        match output
            .recv()
            .expect("Unable to receive data from JavaScript thread")
        {
            ProcessorOutput::Start(res) => res.map(|_| JavaScript { input, output }),
            _ => unreachable!(),
        }
    }
}

impl Drop for JavaScript {
    fn drop(&mut self) {
        self.input
            .send(ProcessorInput::Stop)
            .expect("Unable to send data to JavaScript thread")
    }
}

impl Transform for JavaScript {
    // only used in tests
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut output = Vec::new();
        self.transform_into(&mut output, event);
        assert!(output.len() <= 1);
        output.pop()
    }

    fn transform_into(&mut self, output_events: &mut Vec<Event>, event: Event) {
        self.input
            .send(ProcessorInput::Event(event))
            .expect("Unable to send data to JavaScript thread");
        match self
            .output
            .recv()
            .expect("Unable to receive message from a JavaScriptProcessor thread")
        {
            ProcessorOutput::Events(mut transformed) => output_events.append(&mut transformed),
            _ => unreachable!(),
        }
    }
}

struct JavaScriptProcessor {
    ctx: Context,
}

impl JavaScriptProcessor {
    pub fn new(config: JavaScriptConfig) -> crate::Result<Self> {
        // load source
        let source = match config.source_or_path {
            SourceOrPath::Source { source } => source,
            SourceOrPath::Path { path } => {
                fs::read_to_string(&path).context(JavascriptLoadSourceError { path })?
            }
        };

        // validate handler parameter if present
        if let Some(ref handler) = config.handler {
            if !JS_VALID_IDENTIFIER.is_match(&handler) {
                return Err(Box::new(
                    BuildError::JavascriptHandlerIsNotIdentifierError {
                        handler: handler.to_string(),
                    },
                ));
            }
            if JS_RESERVED_KEYWORDS.contains(&handler[..]) {
                return Err(Box::new(BuildError::JavascriptHandlerReservedError {
                    handler: handler.to_string(),
                }));
            }
        }

        // init QuickJS context
        let mut builder = Context::builder();
        if let Some(memory_limit) = config.memory_limit {
            builder = builder.memory_limit(memory_limit);
        }

        let ctx = builder.build().context(JavascriptRuntimeCreationError)?;

        // inject handler source
        let source = if let Some(handler) = config.handler {
            format!("{}; __vector_handler = {}", source, handler)
        } else {
            format!(r"__vector_handler = ({})", source)
        };
        ctx.eval(&source).context(JavascriptInjectionError)?;

        // check that handler is a function
        let handler_is_a_function: bool = ctx
            .eval_as(r"typeof __vector_handler === 'function'")
            .context(JavascriptValidationError)?;
        if !handler_is_a_function {
            return Err(Box::new(
                BuildError::JavascriptHandlerIsNotAFunctionError {},
            ));
        }

        Ok(Self { ctx })
    }

    pub fn process(&self, output: &mut Vec<Event>, event: Event) -> crate::Result<()> {
        let encoded = encode(event);
        let transformed = self
            .ctx
            .call_function("__vector_handler", vec![encoded])
            .context(JavascriptRuntimeError)?;
        write_value(transformed, output)
    }
}

fn encode(event: Event) -> JsValue {
    let js_event = event
        .as_log()
        .all_fields()
        .map(|(key, value)| {
            (
                key.to_string(),
                match value {
                    ValueKind::Bytes(v) => JsValue::String(String::from_utf8_lossy(v).to_string()),
                    ValueKind::Integer(v) => JsValue::BigInt((*v).into()),
                    ValueKind::Float(v) => JsValue::Float(*v),
                    ValueKind::Boolean(v) => JsValue::Bool(*v),
                    ValueKind::Timestamp(v) => JsValue::Date(*v),
                },
            )
        })
        .collect();
    JsValue::Object(js_event)
}

fn write_value(value: JsValue, output: &mut Vec<Event>) -> crate::Result<()> {
    match value {
        JsValue::Array(js_events) => {
            for js_event in js_events {
                write_event(js_event, output)?;
            }
            Ok(())
        }
        _ => write_event(value, output),
    }
}

fn write_event(js_event: JsValue, output: &mut Vec<Event>) -> crate::Result<()> {
    match js_event {
        JsValue::Null => Ok(()),
        JsValue::Object(object) => {
            let event = object_to_event(object)?;
            output.push(event);
            Ok(())
        }
        _ => Err(Box::new(ProcessError::JavascriptUnexpectedValueError {
            js_event,
        })),
    }
}

fn object_to_event(object: HashMap<String, JsValue>) -> crate::Result<Event> {
    let mut event = Event::new_empty_log();
    let log = event.as_mut_log();
    for (k, v) in object.into_iter() {
        let v =
            match v {
                JsValue::Null => continue,
                JsValue::Bool(v) => ValueKind::Boolean(v),
                JsValue::BigInt(v) => {
                    let int = v.as_i64().ok_or(Box::new(
                        ProcessError::JavascriptBigintOverflowError { value: v },
                    ))?;
                    ValueKind::Integer(int)
                }
                JsValue::Int(v) => ValueKind::Float(v as f64),
                JsValue::Float(v) => ValueKind::Float(v),
                JsValue::String(v) => ValueKind::Bytes(v.into()),
                JsValue::Date(v) => ValueKind::Timestamp(v),
                JsValue::Object(v) => {
                    return Err(Box::new(ProcessError::JavascriptNestedObjectsError {
                        field: k,
                        value: v,
                    }));
                }
                JsValue::Array(v) => {
                    return Err(Box::new(ProcessError::JavascriptNestedArraysError {
                        field: k,
                        value: v,
                    }))
                }
            };
        log.insert_implicit(k.into(), v.into());
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::{JavaScript, JavaScriptConfig};
    use crate::{
        event::{Event, ValueKind, TIMESTAMP},
        transforms::Transform,
    };
    use chrono::{TimeZone, Utc};
    use mktemp::Temp;
    use std::fs;

    #[test]
    fn javascript_new() {
        let config = r#"
            source = "event => event"
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_path() {
        let path = Temp::new_path();
        let path_str = path.to_str().unwrap();
        fs::write(path_str, r"event => event").unwrap();
        let config = format!(
            r#"
            path = "{}"
            "#,
            path_str
        );
        let res = JavaScript::new(toml::from_str(&config).unwrap());
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_source_and_path() {
        let source = r"event => event";

        let path = Temp::new_path();
        let path_str = path.to_str().unwrap();
        fs::write(path_str, source).unwrap();
        let config = format!(
            r#"
            path = "{}"
            source = "{}"
            "#,
            path_str, source
        );
        let res = toml::from_str::<JavaScriptConfig>(&config);
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_without_source_and_path() {
        let config = "";
        let res = toml::from_str::<JavaScriptConfig>(&config);
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_syntax_error() {
        let config = r#"
            source = "..."
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_not_a_function() {
        let config = r#"
            source = "123"
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_with_handler_function() {
        let config = r#"
            source = """
                function handler(event) {
                    return event
                }
            """
            handler = "handler"
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_handler_const() {
        let config = r#"
            source = """
                const handler = event => event
            """
            handler = "handler"
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_handler_global_object_property() {
        let config = r#"
            source = """
                handler = event => event
            """
            handler = "handler"
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_handler_missing() {
        let config = r#"
            source = """
                const handler1 = event => event
            """
            handler = "handler2"
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_with_handler_not_a_valid_identifier() {
        let config = r#"
            source = """
                event => event
            """
            handler = "!@#$"
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_with_handler_reserved_keyword() {
        let config = r#"
            source = """
                event => event
            """
            handler = "new"
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_err());
    }

    #[test]
    fn javascript_new_with_memory_limit_success() {
        let config = r#"
            source = """
                event => event
            """
            memory_limit = 10000000
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_ok());
    }

    #[test]
    fn javascript_new_with_memory_limit_failure_oom() {
        let config = r#"
            source = """
                event => event
            """
            memory_limit = 10
        "#;
        let res = JavaScript::new(toml::from_str(config).unwrap());
        assert!(res.is_err());
    }

    fn make_js(source: &str) -> JavaScript {
        let config = toml! {
            source = source
        }
        .try_into()
        .unwrap();
        JavaScript::new(config).unwrap()
    }

    fn make_js_with_handler(source: &str, handler: &str) -> JavaScript {
        let config = toml! {
            source = source
            handler = handler
        }
        .try_into()
        .unwrap();
        JavaScript::new(config).unwrap()
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
            event => ({...event, field: 123n})
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
    fn javascript_transform_add_field_float_without_decimals() {
        let mut js = make_js(
            r#"
            event => ({...event, field: 123})
            "#,
        );

        let source_event = make_event();

        let mut expected_event = source_event.clone();
        let expected_log = expected_event.as_mut_log();
        expected_log.insert_implicit("field".into(), 123.0.into());

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
            event => ({...event, field: {a: 3n, b: 4n}})
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
            event => ({...event, field: [1n,2n,3n]})
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
            event => [{...event, a: 3n}, {...event, b: 4n}]
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
            let count = 0n
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
