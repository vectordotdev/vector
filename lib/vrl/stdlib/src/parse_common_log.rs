use std::collections::BTreeMap;

use ::value::Value;
use vrl::prelude::*;

use crate::log_util;

fn parse_common_log(bytes: Value, timestamp_format: Option<Value>, ctx: &Context) -> Resolved {
    let message = bytes.try_bytes_utf8_lossy()?;
    let timestamp_format = match timestamp_format {
        None => "%d/%b/%Y:%T %z".to_owned(),
        Some(timestamp_format) => timestamp_format.try_bytes_utf8_lossy()?.to_string(),
    };

    log_util::parse_message(
        &log_util::REGEX_APACHE_COMMON_LOG,
        &message,
        &timestamp_format,
        ctx.timezone(),
        "common",
    )
    .map_err(Into::into)
}

#[derive(Clone, Copy, Debug)]
pub struct ParseCommonLog;

impl Function for ParseCommonLog {
    fn identifier(&self) -> &'static str {
        "parse_common_log"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "timestamp_format",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let timestamp_format = arguments.optional("timestamp_format");

        Ok(ParseCommonLogFn {
            value,
            timestamp_format,
        }
        .as_expr())
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse common log",
            source: r#"parse_common_log!(s'127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326')"#,
            result: Ok(indoc! {
                r#"{
                    "host":"127.0.0.1",
                    "identity":"bob",
                    "message":"GET /apache_pb.gif HTTP/1.0",
                    "method":"GET",
                    "path":"/apache_pb.gif",
                    "protocol":"HTTP/1.0",
                    "size":2326,
                    "status":200,
                    "timestamp":"2000-10-10T20:55:36Z",
                    "user":"frank"
                }"#
            }),
        }]
    }
}

#[derive(Debug, Clone)]
struct ParseCommonLogFn {
    value: Box<dyn Expression>,
    timestamp_format: Option<Box<dyn Expression>>,
}

impl FunctionExpression for ParseCommonLogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        let timestamp_format = self
            .timestamp_format
            .as_ref()
            .map(|expr| expr.resolve(ctx))
            .transpose()?;

        parse_common_log(bytes, timestamp_format, ctx)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(inner_kind()).fallible()
    }
}

fn inner_kind() -> BTreeMap<Field, Kind> {
    BTreeMap::from([
        (Field::from("host"), Kind::bytes() | Kind::null()),
        (Field::from("identity"), Kind::bytes() | Kind::null()),
        (Field::from("user"), Kind::bytes() | Kind::null()),
        (Field::from("timestamp"), Kind::timestamp() | Kind::null()),
        (Field::from("message"), Kind::bytes() | Kind::null()),
        (Field::from("method"), Kind::bytes() | Kind::null()),
        (Field::from("path"), Kind::bytes() | Kind::null()),
        (Field::from("protocol"), Kind::bytes() | Kind::null()),
        (Field::from("status"), Kind::integer() | Kind::null()),
        (Field::from("size"), Kind::integer() | Kind::null()),
    ])
}

#[cfg(test)]
mod tests {
    use chrono::prelude::*;
    use vector_common::btreemap;

    use super::*;

    test_function![
        parse_common_log => ParseCommonLog;

        log_line_valid {
            args: func_args![value: r#"127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326"#],
            want: Ok(btreemap! {
                "host" => "127.0.0.1",
                "identity" => "bob",
                "user" => "frank",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2000-10-10T20:55:36Z").unwrap().into()),
                "message" => "GET /apache_pb.gif HTTP/1.0",
                "method" => "GET",
                "path" => "/apache_pb.gif",
                "protocol" => "HTTP/1.0",
                "status" => 200,
                "size" => 2326,
            }),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        log_line_valid_empty {
            args: func_args![value: "- - - - - - -"],
            want: Ok(BTreeMap::new()),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        log_line_valid_empty_variant {
            args: func_args![value: r#"- - - [-] "-" - -"#],
            want: Ok(BTreeMap::new()),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        log_line_valid_with_timestamp_format {
            args: func_args![value: r#"- - - [2000-10-10T20:55:36Z] "-" - -"#,
                             timestamp_format: "%+",
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2000-10-10T20:55:36Z").unwrap().into()),
            }),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        log_line_invalid {
            args: func_args![value: r#"not a common log line"#],
            want: Err("failed parsing common log line"),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        log_line_invalid_timestamp {
            args: func_args![value: r#"- - - [1234] - - -"#],
            want: Err("failed parsing timestamp 1234 using format %d/%b/%Y:%T %z: input contains invalid characters"),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }
    ];
}
