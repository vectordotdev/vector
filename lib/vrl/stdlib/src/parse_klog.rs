use std::collections::BTreeMap;

use chrono::{offset::TimeZone, Datelike, Utc};
use lazy_static::lazy_static;
use regex::Regex;
use vrl::prelude::*;

lazy_static! {
    static ref REGEX_KLOG: Regex = Regex::new(
        r#"(?x)                                                        # Ignore whitespace and comments in the regex expression.
        ^\s*                                                           # Start with any number of whitespaces.
        (?P<level>\w)                                                  # Match one word character (expecting `I`,`W`,`E` or `F`).
        (?P<timestamp>(?P<month>\d{2})\d{2}\s\d{2}:\d{2}:\d{2}\.\d{6}) # Match MMDD hh:mm:ss.ffffff.
        \s+                                                            # Match one whitespace.
        (?P<id>\d+)                                                    # Match at least one digit.
        \s                                                             # Match one whitespace.
        (?P<file>.+):(?P<line>\d+)                                     # Match any character (greedily), ended by `:` and at least one digit.
        \]\s                                                           # Match `]` and one whitespace.
        (?P<message>.*?)                                               # Match any characters (non-greedily).
        \s*$                                                           # Match any number of whitespaces to be stripped from the end.
    "#)
    .expect("failed compiling regex for klog");
}

#[derive(Clone, Copy, Debug)]
pub struct ParseKlog;

impl Function for ParseKlog {
    fn identifier(&self) -> &'static str {
        "parse_klog"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: r#"parse_klog!("I0505 17:59:40.692994   28133 klog.go:70] hello from klog")"#,
            result: Ok(indoc! { r#"{
                    "file": "klog.go",
                    "id": 28133,
                    "level": "info",
                    "line": 70,
                    "message": "hello from klog",
                    "timestamp": "2022-05-05T17:59:40.692994Z"
                }"#}),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ParseKlogFn { value }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }
}

#[derive(Debug, Clone)]
struct ParseKlogFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseKlogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.try_bytes()?;
        let message = String::from_utf8_lossy(&bytes);

        let mut log: BTreeMap<String, Value> = BTreeMap::new();

        let captures = REGEX_KLOG
            .captures(&message)
            .ok_or("failed parsing klog message")?;

        if let Some(level) = captures.name("level").map(|capture| capture.as_str()) {
            let level = match level {
                "I" => Ok("info"),
                "W" => Ok("warning"),
                "E" => Ok("error"),
                "F" => Ok("fatal"),
                _ => Err(format!(r#"unrecognized log level "{}""#, level)),
            }?;

            log.insert("level".into(), Value::Bytes(level.to_owned().into()));
        }

        if let Some(timestamp) = captures.name("timestamp").map(|capture| capture.as_str()) {
            let month = captures.name("month").map(|capture| capture.as_str());
            let year = resolve_year(month);
            log.insert(
                "timestamp".into(),
                Value::Timestamp(
                    Utc.datetime_from_str(&format!("{}{}", year, timestamp), "%Y%m%d %H:%M:%S%.f")
                        .map_err(|error| {
                            format!(r#"failed parsing timestamp {}: {}"#, timestamp, error)
                        })?,
                ),
            );
        }

        if let Some(id) = captures.name("id").map(|capture| capture.as_str()) {
            log.insert(
                "id".into(),
                Value::Integer(id.parse().map_err(|_| "failed parsing id")?),
            );
        }

        if let Some(file) = captures.name("file").map(|capture| capture.as_str()) {
            log.insert("file".into(), Value::Bytes(file.to_owned().into()));
        }

        if let Some(line) = captures.name("line").map(|capture| capture.as_str()) {
            log.insert(
                "line".into(),
                Value::Integer(line.parse().map_err(|_| "failed parsing line")?),
            );
        }

        if let Some(message) = captures.name("message").map(|capture| capture.as_str()) {
            log.insert("message".into(), Value::Bytes(message.to_owned().into()));
        }

        Ok(log.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object::<&str, Kind>(type_def())
    }
}

// same logic as our handling of RFC3164 syslog messages: since we don't know the year, we look at
// the month to guess the year based on the current month
fn resolve_year(month: Option<&str>) -> i32 {
    let now = Utc::now();
    match (month, now.month()) {
        (Some("12"), 1) => now.year() - 1,
        (_, _) => now.year(),
    }
}

fn type_def() -> BTreeMap<&'static str, Kind> {
    map! {
        "level": Kind::Bytes,
        "timestamp": Kind::Timestamp,
        "id": Kind::Integer,
        "file": Kind::Bytes,
        "line": Kind::Integer,
        "message": Kind::Bytes,
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use vector_common::btreemap;

    use super::*;

    test_function![
        parse_klog => ParseKlog;

        log_line_valid {
            args: func_args![value: "I0505 17:59:40.692994   28133 klog.go:70] hello from klog"],
            want: Ok(btreemap! {
                "level" => "info",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339(&format!("{}-05-05T17:59:40.692994Z", Utc::now().year())).unwrap().into()),
                "id" => 28133,
                "file" => "klog.go",
                "line" => 70,
                "message" => "hello from klog",
            }),
            tdef: TypeDef::new().fallible().object::<&str, Kind>(type_def()),
        }

        log_line_valid_strip_whitespace {
            args: func_args![value: "\n     I0505 17:59:40.692994   28133 klog.go:70] hello from klog    \n"],
            want: Ok(btreemap! {
                "level" => "info",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339(&format!("{}-05-05T17:59:40.692994Z", Utc::now().year())).unwrap().into()),
                "id" => 28133,
                "file" => "klog.go",
                "line" => 70,
                "message" => "hello from klog",
            }),
            tdef: TypeDef::new().fallible().object::<&str, Kind>(type_def()),
        }

        log_line_invalid {
            args: func_args![value: "not a klog line"],
            want: Err("failed parsing klog message"),
            tdef: TypeDef::new().fallible().object::<&str, Kind>(type_def()),
        }

        log_line_invalid_log_level {
            args: func_args![value: "X0505 17:59:40.692994   28133 klog.go:70] hello from klog"],
            want: Err(r#"unrecognized log level "X""#),
            tdef: TypeDef::new().fallible().object::<&str, Kind>(type_def()),
        }

        log_line_invalid_timestamp {
            args: func_args![value: "I0000 17:59:40.692994   28133 klog.go:70] hello from klog"],
            want: Err("failed parsing timestamp 0000 17:59:40.692994: input is out of range"),
            tdef: TypeDef::new().fallible().object::<&str, Kind>(type_def()),
        }

        log_line_invalid_id {
            args: func_args![value: "I0505 17:59:40.692994   99999999999999999999999999999 klog.go:70] hello from klog"],
            want: Err("failed parsing id"),
            tdef: TypeDef::new().fallible().object::<&str, Kind>(type_def()),
        }
    ];
}
