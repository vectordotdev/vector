use std::collections::BTreeMap;

use vrl::prelude::*;

use crate::log_util;

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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let timestamp_format = arguments.optional("timestamp_format");

        Ok(Box::new(ParseCommonLogFn {
            value,
            timestamp_format,
        }))
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

impl Expression for ParseCommonLogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        let message = bytes.try_bytes_utf8_lossy()?;
        let timestamp_format = match &self.timestamp_format {
            None => "%d/%b/%Y:%T %z".to_owned(),
            Some(timestamp_format) => timestamp_format
                .resolve(ctx)?
                .try_bytes_utf8_lossy()?
                .to_string(),
        };

        let captures = log_util::REGEX_APACHE_COMMON_LOG
            .captures(&message)
            .ok_or("failed parsing common log line")?;

        log_util::log_fields(
            &log_util::REGEX_APACHE_COMMON_LOG,
            &captures,
            &timestamp_format,
            ctx.timezone(),
        )
        .map_err(Into::into)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object(type_def())
    }
}

fn type_def() -> BTreeMap<&'static str, TypeDef> {
    map! {
        "host": Kind::Bytes | Kind::Null,
        "identity": Kind::Bytes | Kind::Null,
        "user": Kind::Bytes | Kind::Null,
        "timestamp": Kind::Timestamp | Kind::Null,
        "message": Kind::Bytes | Kind::Null,
        "method": Kind::Bytes | Kind::Null,
        "path": Kind::Bytes | Kind::Null,
        "protocol": Kind::Bytes | Kind::Null,
        "status": Kind::Integer | Kind::Null,
        "size": Kind::Integer | Kind::Null,
    }
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
            tdef: TypeDef::new().fallible().object(type_def()),
        }

        log_line_valid_empty {
            args: func_args![value: "- - - - - - -"],
            want: Ok(btreemap! {}),
            tdef: TypeDef::new().fallible().object(type_def()),
        }

        log_line_valid_empty_variant {
            args: func_args![value: r#"- - - [-] "-" - -"#],
            want: Ok(btreemap! {}),
            tdef: TypeDef::new().fallible().object(type_def()),
        }

        log_line_valid_with_timestamp_format {
            args: func_args![value: r#"- - - [2000-10-10T20:55:36Z] "-" - -"#,
                             timestamp_format: "%+",
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2000-10-10T20:55:36Z").unwrap().into()),
            }),
            tdef: TypeDef::new().fallible().object(type_def()),
        }

        log_line_invalid {
            args: func_args![value: r#"not a common log line"#],
            want: Err("failed parsing common log line"),
            tdef: TypeDef::new().fallible().object(type_def()),
        }

        log_line_invalid_timestamp {
            args: func_args![value: r#"- - - [1234] - - -"#],
            want: Err("failed parsing timestamp 1234 using format %d/%b/%Y:%T %z: input contains invalid characters"),
            tdef: TypeDef::new().fallible().object(type_def()),
        }
    ];
}
