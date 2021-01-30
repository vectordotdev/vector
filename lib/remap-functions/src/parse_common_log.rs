use chrono::DateTime;
use lazy_static::lazy_static;
use regex::Regex;
use remap::prelude::*;
use std::collections::BTreeMap;

lazy_static! {
    // Information about the common log format taken from the
    // - W3C specification: https://www.w3.org/Daemon/User/Config/Logging.html#common-logfile-format
    // - Apache HTTP Server docs: https://httpd.apache.org/docs/1.3/logs.html#common
    static ref REGEX_COMMON_LOG: Regex = Regex::new(
        r#"(?x)                                 # Ignore whitespace and comments in the regex expression.
        ^\s*                                    # Start with any number of whitespaces.
        (-|(?P<remote_host>.*?))\s+             # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|(?P<remote_logname>.*?))\s+          # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|(?P<auth_user>.*?))\s+               # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|\[(-|(?P<timestamp>[^\[]*))\])\s+    # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.
        (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
        (?P<request_line>(                      # Match a request with...
        (?P<request_method>\w+)\s+              # Match at least one word character and at least one whitespace.
        (?P<request_path>[[\\"][^"]]*?)\s+      # Match any character except `"`, but `\"` (non-greedily) and at least one whitespace.
        (?P<request_protocol>[[\\"][^"]]*?)\s*  # Match any character except `"`, but `\"` (non-greedily) and any number of whitespaces.
        |[[\\"][^"]]*?))\s*))"                  # ...Or match any charater except `"`, but `\"`, and any amount of whitespaces.
        )\s+                                    # Match at least one whitespace.
        (-|(?P<status_code>\d+))\s+             # Match `-` or at least one digit and at least one whitespace.
        (-|(?P<content_length>\d+))             # Match `-` or at least one digit.
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
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "timestamp_format",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let timestamp_format = arguments.optional_literal("timestamp_format")?.map_or(
            Ok("%d/%b/%Y:%T %z".into()),
            |literal| {
                literal
                    .as_value()
                    .clone()
                    .try_bytes_utf8_lossy()
                    .map(|bytes| bytes.into_owned())
            },
        )?;

        Ok(Box::new(ParseCommonLogFn {
            value,
            timestamp_format,
        }))
    }
}

#[derive(Debug, Clone)]
struct ParseCommonLogFn {
    value: Box<dyn Expression>,
    timestamp_format: String,
}

impl Expression for ParseCommonLogFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let message = String::from_utf8_lossy(&bytes);

        let mut log: BTreeMap<String, Value> = BTreeMap::new();

        let captures = REGEX_COMMON_LOG
            .captures(&message)
            .ok_or("failed parsing common log line")?;

        if let Some(remote_host) = captures.name("remote_host").map(|capture| capture.as_str()) {
            log.insert(
                "remote_host".into(),
                Value::Bytes(remote_host.to_owned().into()),
            );
        }

        if let Some(remote_logname) = captures
            .name("remote_logname")
            .map(|capture| capture.as_str())
        {
            log.insert(
                "remote_logname".into(),
                Value::Bytes(remote_logname.to_owned().into()),
            );
        }

        if let Some(auth_user) = captures.name("auth_user").map(|capture| capture.as_str()) {
            log.insert(
                "auth_user".into(),
                Value::Bytes(auth_user.to_owned().into()),
            );
        }

        if let Some(timestamp) = captures.name("timestamp").map(|capture| capture.as_str()) {
            log.insert(
                "timestamp".into(),
                Value::Timestamp(
                    DateTime::parse_from_str(timestamp, &self.timestamp_format)
                        .map_err(|error| {
                            format!(
                                r#"failed parsing timestamp {} using format {}: {}"#,
                                timestamp, self.timestamp_format, error
                            )
                        })?
                        .into(),
                ),
            );
        }

        if let Some(request_line) = captures
            .name("request_line")
            .map(|capture| capture.as_str())
        {
            log.insert(
                "request_line".into(),
                Value::Bytes(request_line.to_owned().into()),
            );
        }

        if let Some(request_method) = captures
            .name("request_method")
            .map(|capture| capture.as_str())
        {
            log.insert(
                "request_method".into(),
                Value::Bytes(request_method.to_owned().into()),
            );
        }

        if let Some(request_path) = captures
            .name("request_path")
            .map(|capture| capture.as_str())
        {
            log.insert(
                "request_path".into(),
                Value::Bytes(request_path.to_owned().into()),
            );
        }

        if let Some(request_protocol) = captures
            .name("request_protocol")
            .map(|capture| capture.as_str())
        {
            log.insert(
                "request_protocol".into(),
                Value::Bytes(request_protocol.to_owned().into()),
            );
        }

        if let Some(status_code) = captures.name("status_code").map(|capture| capture.as_str()) {
            log.insert(
                "status_code".into(),
                Value::Integer(
                    status_code
                        .parse()
                        .map_err(|_| "failed parsing status code")?,
                ),
            );
        }

        if let Some(content_length) = captures
            .name("content_length")
            .map(|capture| capture.as_str())
        {
            log.insert(
                "content_length".into(),
                Value::Integer(
                    content_length
                        .parse()
                        .map_err(|_| "failed parsing content length")?,
                ),
            );
        }

        Ok(log.into())
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
    use shared::btreemap;

    test_function![
        parse_common_log => ParseCommonLog;

        log_line_valid {
            args: func_args![value: r#"127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326"#],
            want: Ok(btreemap! {
                "remote_host" => "127.0.0.1",
                "remote_logname" => "bob",
                "auth_user" => "frank",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2000-10-10T20:55:36Z").unwrap().into()),
                "request_line" => "GET /apache_pb.gif HTTP/1.0",
                "request_method" => "GET",
                "request_path" => "/apache_pb.gif",
                "request_protocol" => "HTTP/1.0",
                "status_code" => 200,
                "content_length" => 2326,
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
            expr: |_| ParseCommonLogFn { value: Literal::from("foo").boxed(), timestamp_format: "".into() },
            def: TypeDef { kind: value::Kind::Map, ..Default::default() },
        }

        value_non_string {
            expr: |_| ParseCommonLogFn { value: Literal::from(1).boxed(), timestamp_format: "".into() },
            def: TypeDef { fallible: true, kind: value::Kind::Map, ..Default::default() },
        }

        value_optional {
            expr: |_| ParseCommonLogFn { value: Box::new(Noop), timestamp_format: "".into() },
            def: TypeDef { fallible: true, kind: value::Kind::Map, ..Default::default() },
        }
    ];
}
