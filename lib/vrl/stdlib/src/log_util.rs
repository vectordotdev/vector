use std::collections::BTreeMap;

use ::value::Value;
use chrono::prelude::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use vector_common::TimeZone;

// Information about the common log format taken from the
// - W3C specification: https://www.w3.org/Daemon/User/Config/Logging.html#common-logfile-format
// - Apache HTTP Server docs: https://httpd.apache.org/docs/1.3/logs.html#common
#[cfg(any(feature = "parse_apache_log", feature = "parse_common_log"))]
pub(crate) static REGEX_APACHE_COMMON_LOG: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(
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
            |[[\\"][^"]]*?))\s*))"                  # ...Or match any character except `"`, but `\"`, and any amount of whitespaces.
            )\s+                                    # Match at least one whitespace.
            (-|(?P<status>\d+))\s+                  # Match `-` or at least one digit and at least one whitespace.
            (-|(?P<size>\d+))                       # Match `-` or at least one digit.
            \s*$                                    # Match any number of whitespaces (to be discarded).
        "#)
        .expect("failed compiling regex for common log")
    ]
});

// - Apache HTTP Server docs: https://httpd.apache.org/docs/1.3/logs.html#combined
#[cfg(feature = "parse_apache_log")]
pub(crate) static REGEX_APACHE_COMBINED_LOG: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(
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
            |[[\\"][^"]]*?))\s*))"                  # ...Or match any character except `"`, but `\"`, and any amount of whitespaces.
            )\s+                                    # Match at least one whitespace.
            (-|(?P<status>\d+))\s+                  # Match `-` or at least one digit and at least one whitespace.
            (-|(?P<size>\d+))\s+                    # Match `-` or at least one digit.
            (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
            (?P<referrer>[[\\"][^"]]*?)             # Match any character except `"`, but `\"`
            ")))                                    # Match the closing quote
            \s+                                     # Match whitespace
            (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
            (?P<agent>[[\\"][^"]]*?)                # Match any character except `"`, but `\"`
            ")))                                    # Match the closing quote
            #\s*$                                   # Match any number of whitespaces (to be discarded).
        "#)
        .expect("failed compiling regex for combined log")
    ]
});

// It is possible to customise the format output by apache.
#[cfg(feature = "parse_apache_log")]
pub(crate) static REGEX_APACHE_ERROR_LOG: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // Simple format
        // https://github.com/mingrammer/flog/blob/9bc83b14408ca446e934c32e4a88a81a46e78d83/log.go#L16
        Regex::new(
            r#"(?x)                                     # Ignore whitespace and comments in the regex expression.
            ^\s*                                        # Start with any number of whitespaces.
            (-|\[(-|(?P<timestamp>[^\[]*))\])\s+        # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.
            (-|\[(-|(?P<module>[^:]*):                  # Match `-` or `[` followed by `-` or any character except `:`.
            (?P<severity>[^\[]*))\])\s+                 # Match ary character except `]`, `]` and at least one whitespace.
            (-|\[\s*pid\s*(-|(?P<pid>[^:]*)             # Match `-` or `[` followed by `pid`, `-` or any character except `:`.
            (:\s*tid\s*(?P<thread>[^\[]*))?)\])\s       # Match `tid` followed by any character except `]`, `]` and at least one whitespace.
            (-|\[\s*client\s*(-|(?P<client>.*:?):       # Match `-` or `[` followed by `client`, `-` or any character until the first or last `:` for the port.
            (?P<port>[^\[]*))\])\s                      # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.
            (-|(?P<message>.*))                         # Match `-` or any character.
            \s*$                                        # Match any number of whitespaces (to be discarded).
        "#)
        .expect("failed compiling regex for error log"),

        // threaded MPM format
        // https://httpd.apache.org/docs/current/mod/core.html#errorlogformat
        Regex::new(
            r#"(?x)                                              # Ignore whitespace and comments in the regex expression.
            ^\s*                                                 # Start with any number of whitespaces.
            \[(?P<timestamp>[^\]]+)\]\s+                         # [%{u}t]
            \[(-|(?P<module>[^:]+)):(?P<severity>[^\]]+)\]\s+    # [%-m:%l]
            \[pid\s+(?P<pid>\d+)(:tid\s+(?P<thread>\d+))?\]\s+   # [pid %P:tid %T]
            (?P<message1>[^\[]*?:\s+([^\[]*?:\s+)?)?             # %7F: %E:
            (\[client\s+(?P<client>.+?):(?P<port>\d+)\]\s+)?     # [client\ %a]
            (?P<message2>.*)                                     # %M
            (, referer .*)?                                      # ,\ referer\ %{Referer}
            \s*$                                                 # Match any number of whitespaces (to be discarded).
        "#)
        .expect("failed compiling regex for error log")
    ]
});

// - Nginx HTTP Server docs: http://nginx.org/en/docs/http/ngx_http_log_module.html
#[cfg(feature = "parse_nginx_log")]
pub(crate) static REGEX_NGINX_COMBINED_LOG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?x)                                 # Ignore whitespace and comments in the regex expression.
        ^\s*                                    # Start with any number of whitespaces.
        (-|(?P<client>\S+))\s+                  # Match `-` or any non space character
        \-\s+                                   # Always a dash
        (-|(?P<user>\S+))\s+                    # Match `-` or any non space character
        \[(?P<timestamp>.+)\]\s+                # Match date between brackets
        "(?P<request>
        (?P<method>\w+)\s+                      # Match at least a word
        (?P<path>\S+)\s+                        # Match any non space character
        (?P<protocol>\S+)
        )"\s+                                   # Match any non space character
        (?P<status>\d+)\s+                      # Match numbers
        (?P<size>\d+)\s+                        # Match numbers
        "(-|(?P<referer>[^"]+))"\s+             # Match `-` or any non double-quote character
        "(-|(?P<agent>[^"]+))"                  # Match `-` or any non double-quote character
        (\s+"(-|(?P<compression>[^"]+))")?      # Match `-` or any non double-quote character
        \s*$                                    # Match any number of whitespaces (to be discarded).
    "#)
    .expect("failed compiling regex for Nginx combined log")
});

#[cfg(feature = "parse_nginx_log")]
pub(crate) static REGEX_NGINX_ERROR_LOG: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?x)                                                                  # Ignore whitespace and comments in the regex expression.
        ^\s*                                                                     # Start with any number of whitespaces.
        (?P<timestamp>.+)\s+                                                     # Match any character until [
        \[(?P<severity>\w+)\]\s+                                                 # Match any word character
        (?P<pid>\d+)\#                                                           # Match any number
        (?P<tid>\d+):                                                            # Match any number
        (\s+\*(?P<cid>\d+))?                                                     # Match any number
        \s+(?P<message>[^,]*)                                                    # Match any character
        (,\s+excess:\s+(?P<excess>[^\s]+)\sby\szone\s"(?P<zone>[^,]+)")?         # Match any character after ', excess: ' until ' by zone ' and the rest of characters
        (,\s+client:\s+(?P<client>[^,]+))?                                       # Match any character after ', client: '
        (,\s+server:\s+(?P<server>[^,]+))?                                       # Match any character after ', server: '
        (,\s+request:\s+"(?P<request>[^"]+)")?                                   # Match any character after ', request: '
        (,\s+upstream:\s+"(?P<upstream>[^"]+)")?                                 # Match any character after ', upstream: '
        (,\s+host:\s+"(?P<host>[^"]+)")?                                         # Match any character then ':' then any character after ', host: '
        (,\s+refer?rer:\s+"(?P<referer>[^"]+)")?                                 # Match any character after ', referrer: '
        \s*$                                                                     # Match any number of whitespaces (to be discarded).
    "#)
    .expect("failed compiling regex for Nginx error log")
});

// Parse the time as Utc from the given timezone
fn parse_time(
    time: &str,
    format: &str,
    timezone: &TimeZone,
) -> std::result::Result<DateTime<Utc>, String> {
    timezone
        .datetime_from_str(time, format)
        .or_else(|_| DateTime::parse_from_str(time, format).map(Into::into))
        .map_err(|err| {
            format!(
                "failed parsing timestamp {} using format {}: {}",
                time, format, err
            )
        })
}

/// Takes the field as a string and returns a `Value`.
/// Most fields are `Value::Bytes`, but some are other types, we convert to those
/// types based on the fieldname.
fn capture_value(
    name: &str,
    value: &str,
    timestamp_format: &str,
    timezone: &TimeZone,
) -> std::result::Result<Value, String> {
    Ok(match name {
        "timestamp" => Value::Timestamp(parse_time(value, timestamp_format, timezone)?),
        "status" | "size" | "pid" | "tid" | "cid" | "port" => Value::Integer(
            value
                .parse()
                .map_err(|_| format!("failed parsing {name}"))?,
        ),
        "excess" => Value::Float(
            value
                .parse()
                .map_err(|_| format!("failed parsing {name}"))?,
        ),
        _ => Value::Bytes(value.to_owned().into()),
    })
}

/// Extracts the log fields from the regex and adds them to a `Value::Object`.
pub(crate) fn log_fields(
    regex: &Regex,
    captures: &Captures,
    timestamp_format: &str,
    timezone: &TimeZone,
) -> std::result::Result<Value, String> {
    Ok(regex
        .capture_names()
        .filter_map(|name| {
            name.and_then(|name| {
                captures.name(name).map(|value| {
                    Ok((
                        name.to_string(),
                        capture_value(name, value.as_str(), timestamp_format, timezone)?,
                    ))
                })
            })
        })
        .collect::<std::result::Result<BTreeMap<String, Value>, String>>()?
        .into())
}

/// Attempts to extract log fields from each of the list of regexes
pub(crate) fn parse_message(
    regexes: &Vec<Regex>,
    message: &str,
    timestamp_format: &str,
    timezone: &TimeZone,
    log_type: &str,
) -> std::result::Result<Value, String> {
    for regex in regexes {
        if let Some(captures) = regex.captures(message) {
            return log_fields(regex, &captures, timestamp_format, timezone);
        }
    }
    Err(format!("failed parsing {log_type} log line"))
}
