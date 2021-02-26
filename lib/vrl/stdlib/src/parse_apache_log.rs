use chrono::prelude::*;
use chrono::DateTime;
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::BTreeMap;
use vrl::prelude::*;

lazy_static! {
    // Information about the common log format taken from the
    // - W3C specification: https://www.w3.org/Daemon/User/Config/Logging.html#common-logfile-format
    // - Apache HTTP Server docs: https://httpd.apache.org/docs/1.3/logs.html#common

    // ApacheCommonLog : {host} {user-identifier} {auth-user-id} [{datetime}] "{method} {request} {protocol}" {response-code} {bytes}
    // ApacheCommonLog = "%s - %s [%s] \"%s %s %s\" %d %d"
    // ApacheCombinedLog : {host} {user-identifier} {auth-user-id} [{datetime}] "{method} {request} {protocol}" {response-code} {bytes} "{referrer}" "{agent}"
    // ApacheCombinedLog = "%s - %s [%s] \"%s %s %s\" %d %d \"%s\" \"%s\""
    // ApacheErrorLog : [{timestamp}] [{module}:{severity}] [pid {pid}:tid {thread-id}] [client %{client}:{port}] %{message}
    // ApacheErrorLog = "[%s] [%s:%s] [pid %d:tid %d] [client %s:%d] %s"

    static ref REGEX_APACHE_COMMON_LOG: Regex = Regex::new(
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

    static ref REGEX_APACHE_COMBINED_LOG: Regex = Regex::new(
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
        (-|(?P<size>\d+))\s+                    # Match `-` or at least one digit.

        (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
        (?P<referrer>[[\\"][^"]]*?)
        ")))        
        \s+
        (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
        (?P<agent>[[\\"][^"]]*?)
        ")))
        #\s*$                                    # Match any number of whitespaces (to be discarded).
    "#)
    .expect("failed compiling regex for common log");

    static ref REGEX_APACHE_ERROR_LOG: Regex = Regex::new(
        r#"(?x)                                 # Ignore whitespace and comments in the regex expression.
        ^\s*                                    # Start with any number of whitespaces.
        (-|\[(-|(?P<timestamp>[^\[]*))\])\s+    # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.

        (-|\[(-|(?P<module>[^:]*):
        (?P<severity>[^\[]*))\])\s+            # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.

        (-|\[\s*pid\s*(-|(?P<pid>[^:]*):\s*tid\s*
        (?P<thread>[^\[]*))\])\s               # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.

        (-|\[\s*client\s*(-|(?P<client>[^:]*):
        (?P<port>[^\[]*))\])\s               # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.

        (?P<message>.*)

        \s*$                                    # Match any number of whitespaces (to be discarded).
    "#)
    .expect("failed compiling regex for common log");

}

#[derive(Clone, Copy, Debug)]
pub struct ParseApacheLog;

impl Function for ParseApacheLog {
    fn identifier(&self) -> &'static str {
        "parse_apache_log"
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
            Parameter {
                keyword: "format",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let variants = vec![value!("common"), value!("combined"), value!("error")];

        let value = arguments.required("value");
        let format = arguments.required_enum("format", &variants)?.unwrap_bytes();

        let timestamp_format = arguments.optional("timestamp_format");

        Ok(Box::new(ParseApacheLogFn {
            value,
            format,
            timestamp_format,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse apache log",
            source: r#"encode_json(parse_apache_log!(s'127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326'))"#,
            result: Ok(
                indoc! {r#"s'{"host":"127.0.0.1","identity":"bob","message":"GET /apache_pb.gif HTTP/1.0","method":"GET","path":"/apache_pb.gif","protocol":"HTTP/1.0","size":2326,"status":200,"timestamp":"2000-10-10T20:55:36+00:00","user":"frank"}'"#},
            ),
        }]
    }
}

#[derive(Debug, Clone)]
struct ParseApacheLogFn {
    value: Box<dyn Expression>,
    format: Bytes,
    timestamp_format: Option<Box<dyn Expression>>,
}

impl Expression for ParseApacheLogFn {
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

        let captures = match self.format.as_ref() {
            b"common" => REGEX_APACHE_COMMON_LOG
                .captures(&message)
                .ok_or("failed parsing common log line")?,
            b"combined" => REGEX_APACHE_COMBINED_LOG
                .captures(&message)
                .ok_or("failed parsing combined log line")?,
            b"error" => REGEX_APACHE_ERROR_LOG
                .captures(&message)
                .ok_or("failed parsing error log line")?,
            _ => panic!(),
        };

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
                Value::Timestamp(parse_time(&timestamp, &timestamp_format)?),
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

        if let Some(referrer) = captures.name("referrer").map(|capture| capture.as_str()) {
            log.insert("referrer".into(), Value::Bytes(referrer.to_owned().into()));
        }

        if let Some(agent) = captures.name("agent").map(|capture| capture.as_str()) {
            log.insert("agent".into(), Value::Bytes(agent.to_owned().into()));
        }

        if let Some(module) = captures.name("module").map(|capture| capture.as_str()) {
            log.insert("module".into(), Value::Bytes(module.to_owned().into()));
        }

        if let Some(severity) = captures.name("severity").map(|capture| capture.as_str()) {
            log.insert("severity".into(), Value::Bytes(severity.to_owned().into()));
        }

        if let Some(pid) = captures.name("pid").map(|capture| capture.as_str()) {
            log.insert(
                "pid".into(),
                Value::Integer(pid.parse().map_err(|_| "failed parsing pid")?),
            );
        }

        if let Some(thread) = captures.name("thread").map(|capture| capture.as_str()) {
            log.insert("thread".into(), Value::Bytes(thread.to_owned().into()));
        }

        if let Some(client) = captures.name("client").map(|capture| capture.as_str()) {
            log.insert("client".into(), Value::Bytes(client.to_owned().into()));
        }

        if let Some(port) = captures.name("port").map(|capture| capture.as_str()) {
            log.insert("port".into(), Value::Integer(port.parse().map_err(|_| "failed parsing port")?));
        }

        Ok(log.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object(type_def())
    }
}

fn parse_time(time: &str, format: &str) -> std::result::Result<DateTime<Utc>, String> {
    DateTime::parse_from_str(time, &format)
        .map(Into::into)
        .or_else(|_| {
            let parsed =
                &chrono::NaiveDateTime::parse_from_str(time, &format).map_err(|error| {
                    format!(
                        r#"failed parsing timestamp {} using format {}: {}"#,
                        time, format, error
                    )
                })?;

            let result = Local.from_local_datetime(&parsed).earliest();

            match result {
                Some(result) => Ok(result.into()),
                None => Ok(Local.from_utc_datetime(parsed).into()),
            }
        })
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

// combined

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    test_function![
        parse_common_log => ParseApacheLog;

        common_line_valid {
            args: func_args![value: r#"127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326"#,
                             format: "common"
            ],
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

        combined_line_valid {
            args: func_args![value: r#"224.92.49.50 bob frank [25/Feb/2021:12:44:08 +0000] "PATCH /one-to-one HTTP/1.1" 401 84170 "http://www.seniorinfomediaries.com/vertical/channels/front-end/bandwidth" "Mozilla/5.0 (X11; Linux i686; rv:5.0) Gecko/1945-10-12 Firefox/37.0""#,
                             format: "combined"
                             ],
            want: Ok(btreemap! {
                "host" => "224.92.49.50",
                "identity" => "bob",
                "user" => "frank",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-02-25T12:44:08Z").unwrap().into()),
                "message" => "PATCH /one-to-one HTTP/1.1",
                "method" => "PATCH",
                "path" => "/one-to-one",
                "protocol" => "HTTP/1.1",
                "status" => 401,
                "size" => 84170,
                "referrer" => "http://www.seniorinfomediaries.com/vertical/channels/front-end/bandwidth",
                "agent" => "Mozilla/5.0 (X11; Linux i686; rv:5.0) Gecko/1945-10-12 Firefox/37.0",
            }),
            tdef: TypeDef::new().fallible().object(type_def()),
        }

        combined_line_missing_fields_valid {
            args: func_args![value: r#"224.92.49.50 bob frank [25/Feb/2021:12:44:08 +0000] "PATCH /one-to-one HTTP/1.1" 401 84170 - -"#,
                             format: "combined"
                             ],
            want: Ok(btreemap! {
                "host" => "224.92.49.50",
                "identity" => "bob",
                "user" => "frank",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-02-25T12:44:08Z").unwrap().into()),
                "message" => "PATCH /one-to-one HTTP/1.1",
                "method" => "PATCH",
                "path" => "/one-to-one",
                "protocol" => "HTTP/1.1",
                "status" => 401,
                "size" => 84170,
            }),
            tdef: TypeDef::new().fallible().object(type_def()),
        }

        error_line_valid {
            args: func_args![value: r#"[Mon Mar 01 12:00:19 2021] [ab:alert] [pid 4803:tid 3814] [client 147.159.108.175:24259] I'll bypass the haptic COM bandwidth, that should matrix the CSS driver!"#,
                             timestamp_format: "%a %b %d %H:%M:%S %Y",
                             format: "error"
                             ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-03-01T12:00:19Z").unwrap().into()),
                "message" => "I'll bypass the haptic COM bandwidth, that should matrix the CSS driver!",
                "module" => "ab",
                "severity" => "alert",
                "pid" => 4803,
                "thread" => "3814",
                "client" => "147.159.108.175",
                "port" => 24259
            }),
            tdef: TypeDef::new().fallible().object(type_def()),
        }



        /*
        log_line_valid_empty {
            args: func_args![value: "- - - - - - -"],
            want: Ok(btreemap! {}),
            tdef: TypeDef::new().fallible.object(type_def()),
        }

        log_line_valid_empty_variant {
            args: func_args![value: r#"- - - [-] "-" - -"#],
            want: Ok(btreemap! {}),
            tdef: TypeDef::new().fallible.object(type_def()),
        }
        */

        /*
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
            tdef: TypeDef::new().fallible.object(type_def()),
        }
        */

        /*
        log_line_invalid {
            args: func_args![value: r#"not a common log line"#],
            want: Err("function call error: failed parsing common log line"),
            tdef: TypeDef::new().fallible.object(type_def()),
        }

        log_line_invalid_timestamp {
            args: func_args![value: r#"- - - [1234] - - -"#],
            want: Err("function call error: failed parsing timestamp 1234 using format %d/%b/%Y:%T %z: input contains invalid characters"),
            tdef: TypeDef::new().fallible.object(type_def()),
        }
        */
    ];
}
