use chrono::DateTime;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::BTreeMap;
use vrl::prelude::*;

lazy_static! {
    // Information about the common log format taken from the
    // - W3C specification: https://www.w3.org/Daemon/User/Config/Logging.html#common-logfile-format
    // - Apache HTTP Server docs: https://httpd.apache.org/docs/1.3/logs.html#common
    static ref REGEX_COMMON_LOG: Regex = Regex::new(
        r#"(?x)                                 # Ignore whitespace and comments in the regex expression.
        ^\s*                                    # Start with any number of whitespaces.
        (-|(?P<host>.*?))\s+                    # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|(?P<identity>.*?))\s+                # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|(?P<user>.*?))\s+                    # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|\[(-|(?P<timestamp>[^\[]*))\])\s+    # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.
        (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
        (?P<message>(                           # Match a request with...
        (?P<method>\w+)\s+                      # Match at least one word character and at least one whitespace.
        (?P<path>[[\\"][^"]]*?)\s+              # Match any character except `"`, but `\"` (non-greedily) and at least one whitespace.
        (?P<protocol>[[\\"][^"]]*?)\s*          # Match any character except `"`, but `\"` (non-greedily) and any number of whitespaces.
        |[[\\"][^"]]*?))\s*))"                  # ...Or match any charater except `"`, but `\"`, and any amount of whitespaces.
        )\s+                                    # Match at least one whitespace.
        (-|(?P<status>\d+))\s+                  # Match `-` or at least one digit and at least one whitespace.
        (-|(?P<size>\d+))                       # Match `-` or at least one digit.
        \s*$                                    # Match any number of whitespaces (to be discarded).
    "#)
    .expect("failed compiling regex for common log");
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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
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
            source: r#"encode_json(parse_common_log!(s'127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326'))"#,
            result: Ok(
                indoc! {r#"s'{"host":"127.0.0.1","identity":"bob","message":"GET /apache_pb.gif HTTP/1.0","method":"GET","path":"/apache_pb.gif","protocol":"HTTP/1.0","size":2326,"status":200,"timestamp":"2000-10-10T20:55:36+00:00","user":"frank"}'"#},
            ),
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

        let mut log: BTreeMap<String, Value> = BTreeMap::new();

        let captures = REGEX_COMMON_LOG
            .captures(&message)
            .ok_or("failed parsing common log line")?;

        if let Some(host) = captures.name("host").map(|capture| capture.as_str()) {
            log.insert("host".into(), Value::Bytes(host.to_owned().into()));
        }

        if let Some(identity) = captures.name("identity").map(|capture| capture.as_str()) {
            log.insert("identity".into(), Value::Bytes(identity.to_owned().into()));
        }

        if let Some(user) = captures.name("user").map(|capture| capture.as_str()) {
            log.insert("user".into(), Value::Bytes(user.to_owned().into()));
        }

        if let Some(timestamp) = captures.name("timestamp").map(|capture| capture.as_str()) {
            log.insert(
                "timestamp".into(),
                Value::Timestamp(
                    DateTime::parse_from_str(timestamp, &timestamp_format)
                        .map_err(|error| {
                            format!(
                                r#"failed parsing timestamp {} using format {}: {}"#,
                                timestamp, timestamp_format, error
                            )
                        })?
                        .into(),
                ),
            );
        }

        if let Some(message) = captures.name("message").map(|capture| capture.as_str()) {
            log.insert("message".into(), Value::Bytes(message.to_owned().into()));
        }

        if let Some(method) = captures.name("method").map(|capture| capture.as_str()) {
            log.insert("method".into(), Value::Bytes(method.to_owned().into()));
        }

        if let Some(path) = captures.name("path").map(|capture| capture.as_str()) {
            log.insert("path".into(), Value::Bytes(path.to_owned().into()));
        }

        if let Some(protocol) = captures.name("protocol").map(|capture| capture.as_str()) {
            log.insert("protocol".into(), Value::Bytes(protocol.to_owned().into()));
        }

        if let Some(status) = captures.name("status").map(|capture| capture.as_str()) {
            log.insert(
                "status".into(),
                Value::Integer(status.parse().map_err(|_| "failed parsing status code")?),
            );
        }

        if let Some(size) = captures.name("size").map(|capture| capture.as_str()) {
            log.insert(
                "size".into(),
                Value::Integer(size.parse().map_err(|_| "failed parsing content length")?),
            );
        }

        Ok(log.into())
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

/*
#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

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
        }

        log_line_valid_empty {
            args: func_args![value: "- - - - - - -"],
            want: Ok(btreemap! {}),
        }

        log_line_valid_empty_variant {
            args: func_args![value: r#"- - - [-] "-" - -"#],
            want: Ok(btreemap! {}),
        }

        log_line_valid_with_timestamp_format {
            args: {
                let mut args = func_args![value: r#"- - - [2000-10-10T20:55:36Z] "-" - -"#];
                args.insert(
                    "timestamp_format",
                    expression::Argument::new(
                        Box::new(Literal::from("%+").into()),
                        |_| true,
                        "timestamp_format",
                        "parse_common_log",
                    )
                    .into(),
                );
                args
            },
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2000-10-10T20:55:36Z").unwrap().into()),
            }),
        }

        log_line_invalid {
            args: func_args![value: r#"not a common log line"#],
            want: Err("function call error: failed parsing common log line"),
        }

        log_line_invalid_timestamp {
            args: func_args![value: r#"- - - [1234] - - -"#],
            want: Err("function call error: failed parsing timestamp 1234 using format %d/%b/%Y:%T %z: input contains invalid characters"),
        }
    ];

    test_type_def![
        value_string {
            expr: |_| ParseCommonLogFn { value: Literal::from("foo").boxed(), timestamp_format: None },
            def: TypeDef { fallible: true, kind: value::Kind::Map, inner_type_def: Some(inner_type_def()) },
        }

        value_non_string {
            expr: |_| ParseCommonLogFn { value: Literal::from(1).boxed(), timestamp_format: None },
            def: TypeDef { fallible: true, kind: value::Kind::Map, inner_type_def: Some(inner_type_def()) },
        }

        timestamp_format_string {
            expr: |_| ParseCommonLogFn { value: Literal::from("foo").boxed(), timestamp_format: Some(Literal::from("foo").boxed()) },
            def: TypeDef { fallible: true, kind: value::Kind::Map, inner_type_def: Some(inner_type_def()) },
        }

        timestamp_format_non_string {
            expr: |_| ParseCommonLogFn { value: Literal::from("foo").boxed(), timestamp_format: Some(Literal::from(1).boxed()) },
            def: TypeDef { fallible: true, kind: value::Kind::Map, inner_type_def: Some(inner_type_def()) },
        }
    ];
}
*/
