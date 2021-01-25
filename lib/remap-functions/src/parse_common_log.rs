use redeye::parser::{CommonLogLineParser, LogLineParser};
use redeye::types::{LogEvent, LogFieldValue};
use remap::prelude::*;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug)]
pub struct ParseCommonLog;

impl Function for ParseCommonLog {
    fn identifier(&self) -> &'static str {
        "parse_common_log"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ParseCommonLogFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ParseCommonLogFn {
    value: Box<dyn Expression>,
}

fn map_fields_to_value(fields: &HashMap<String, LogFieldValue>) -> Value {
    fields
        .iter()
        .map(|(key, value)| {
            (
                key.into(),
                match value {
                    LogFieldValue::Mapping(mapping) => map_fields_to_value(mapping),
                    LogFieldValue::Timestamp(timestamp) => Value::Timestamp((*timestamp).into()),
                    LogFieldValue::Text(text) => Value::Bytes(text.clone().into()),
                    LogFieldValue::Int(integer) => Value::Integer(*integer as i64),
                },
            )
        })
        .collect::<BTreeMap<_, _>>()
        .into()
}

/// Create a Value::Map from the fields of the given common log event.
fn log_event_to_value(event: LogEvent) -> Value {
    map_fields_to_value(event.fields())
}

impl Expression for ParseCommonLogFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let message = String::from_utf8_lossy(&bytes);

        let parser = CommonLogLineParser::new();
        let parsed = parser
            .parse(&message)
            .map_err(|error| format!("Failed parsing common log line: {}", error))?;

        Ok(log_event_to_value(parsed))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;
    use shared::btreemap;

    test_function![
        parse_common_log => ParseCommonLog;

        log_line_valid {
            args: func_args![value: r#"127.0.0.1 - frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326"#],
            want: Ok(btreemap! {
                "@timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2000-10-10T20:55:36Z").unwrap().into()),
                "@version" => "1",
                "content_length" => 2326,
                "message" => "127.0.0.1 - frank [10/Oct/2000:13:55:36 -0700] \"GET /apache_pb.gif HTTP/1.0\" 200 2326",
                "method" => "GET",
                "protocol" => "HTTP/1.0",
                "remote_host" => "127.0.0.1",
                "remote_user" => "frank",
                "requested_uri" => "/apache_pb.gif",
                "requested_url" => "GET /apache_pb.gif HTTP/1.0",
                "status_code" => 200,
            }),
        }

        log_line_invalid {
            args: func_args![value: r#"foo bar baz"#],
            want: Err("function call error: Failed parsing common log line: Could not parse: foo bar baz"),
        }
    ];

    test_type_def![
        value_string {
            expr: |_| ParseCommonLogFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: value::Kind::Map, ..Default::default() },
        }

        value_non_string {
            expr: |_| ParseCommonLogFn { value: Literal::from(1).boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Map, ..Default::default() },
        }

        value_optional {
            expr: |_| ParseCommonLogFn { value: Box::new(Noop) },
            def: TypeDef { fallible: true, kind: value::Kind::Map, ..Default::default() },
        }
    ];
}
