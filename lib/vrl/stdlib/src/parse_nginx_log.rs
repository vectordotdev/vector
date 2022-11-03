use std::collections::BTreeMap;

use ::value::Value;
use regex::Regex;
use vrl::prelude::*;

use crate::log_util;

fn parse_nginx_log(
    bytes: Value,
    timestamp_format: Option<Value>,
    format: &Bytes,
    ctx: &Context,
) -> Resolved {
    let message = bytes.try_bytes_utf8_lossy()?;
    let timestamp_format = match timestamp_format {
        None => time_format_for_format(format.as_ref()),
        Some(timestamp_format) => timestamp_format.try_bytes_utf8_lossy()?.to_string(),
    };
    let regex = regex_for_format(format.as_ref());
    let captures = regex.captures(&message).ok_or("failed parsing log line")?;
    log_util::log_fields(regex, &captures, &timestamp_format, ctx.timezone())
        .map(rename_referrer)
        .map_err(Into::into)
}

fn variants() -> Vec<Value> {
    vec![value!("combined"), value!("error")]
}

#[derive(Clone, Copy, Debug)]
pub struct ParseNginxLog;

impl Function for ParseNginxLog {
    fn identifier(&self) -> &'static str {
        "parse_nginx_log"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "format",
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
        let format = arguments
            .required_enum("format", &variants())?
            .try_bytes()
            .expect("format not bytes");

        let timestamp_format = arguments.optional("timestamp_format");

        Ok(ParseNginxLogFn {
            value,
            format,
            timestamp_format,
        }
        .as_expr())
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "parse nginx combined log",
                source: r#"encode_json(parse_nginx_log!(s'172.17.0.1 - - [31/Mar/2021:12:04:07 +0000] "GET / HTTP/1.1" 200 612 "-" "curl/7.75.0" "-"', "combined"))"#,
                result: Ok(
                    r#"s'{"agent":"curl/7.75.0","client":"172.17.0.1","method":"GET","path":"/","protocol":"HTTP/1.1","request":"GET / HTTP/1.1","size":612,"status":200,"timestamp":"2021-03-31T12:04:07Z"}'"#,
                ),
            },
            Example {
                title: "parse nginx error log",
                source: r#"encode_json(parse_nginx_log!(s'2021/04/01 13:02:31 [error] 31#31: *1 open() "/usr/share/nginx/html/not-found" failed (2: No such file or directory), client: 172.17.0.1, server: localhost, request: "POST /not-found HTTP/1.1", host: "localhost:8081"', "error"))"#,
                result: Ok(
                    r#"s'{"cid":1,"client":"172.17.0.1","host":"localhost:8081","message":"open() \"/usr/share/nginx/html/not-found\" failed (2: No such file or directory)","pid":31,"request":"POST /not-found HTTP/1.1","server":"localhost","severity":"error","tid":31,"timestamp":"2021-04-01T13:02:31Z"}'"#,
                ),
            },
        ]
    }
}

fn regex_for_format(format: &[u8]) -> &Regex {
    match format {
        b"combined" => &log_util::REGEX_NGINX_COMBINED_LOG,
        b"error" => &log_util::REGEX_NGINX_ERROR_LOG,
        _ => unreachable!(),
    }
}

fn time_format_for_format(format: &[u8]) -> String {
    match format {
        b"combined" => "%d/%b/%Y:%T %z".to_owned(),
        b"error" => "%Y/%m/%d %H:%M:%S".to_owned(),
        _ => unreachable!(),
    }
}

fn rename_referrer(mut value: Value) -> Value {
    if let Some(obj) = value.as_object_mut() {
        if let Some(referer) = obj.remove("referrer") {
            obj.insert("referer".into(), referer);
        }
    }
    value
}

#[derive(Debug, Clone)]
struct ParseNginxLogFn {
    value: Box<dyn Expression>,
    format: Bytes,
    timestamp_format: Option<Box<dyn Expression>>,
}

impl FunctionExpression for ParseNginxLogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        let timestamp_format = self
            .timestamp_format
            .as_ref()
            .map(|expr| expr.resolve(ctx))
            .transpose()?;
        let format = &self.format;

        parse_nginx_log(bytes, timestamp_format, format, ctx)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(match self.format.as_ref() {
            b"combined" => kind_combined(),
            b"error" => kind_error(),
            _ => unreachable!(),
        })
        .fallible()
    }
}

fn kind_combined() -> BTreeMap<Field, Kind> {
    BTreeMap::from([
        ("client".into(), Kind::bytes()),
        ("user".into(), Kind::bytes().or_null()),
        ("timestamp".into(), Kind::timestamp()),
        ("request".into(), Kind::bytes()),
        ("method".into(), Kind::bytes()),
        ("path".into(), Kind::bytes()),
        ("protocol".into(), Kind::bytes()),
        ("status".into(), Kind::integer()),
        ("size".into(), Kind::integer()),
        ("referer".into(), Kind::bytes().or_null()),
        ("agent".into(), Kind::bytes().or_null()),
        ("compression".into(), Kind::bytes().or_null()),
    ])
}

fn kind_error() -> BTreeMap<Field, Kind> {
    BTreeMap::from([
        ("timestamp".into(), Kind::timestamp()),
        ("severity".into(), Kind::bytes()),
        ("pid".into(), Kind::integer()),
        ("tid".into(), Kind::integer()),
        ("cid".into(), Kind::integer()),
        ("message".into(), Kind::bytes()),
        ("excess".into(), Kind::float().or_null()),
        ("zone".into(), Kind::bytes().or_null()),
        ("client".into(), Kind::bytes().or_null()),
        ("server".into(), Kind::bytes().or_null()),
        ("request".into(), Kind::bytes().or_null()),
        ("upstream".into(), Kind::bytes().or_null()),
        ("host".into(), Kind::bytes().or_null()),
        ("port".into(), Kind::bytes().or_null()),
    ])
}

#[cfg(test)]
mod tests {
    use chrono::prelude::*;
    use vector_common::btreemap;

    use super::*;

    test_function![
        parse_combined_log => ParseNginxLog;

        combined_line_valid {
            args: func_args![
                value: r#"172.17.0.1 - - [31/Mar/2021:12:04:07 +0000] "GET / HTTP/1.1" 200 612 "-" "curl/7.75.0" "-""#,
                format: "combined"
            ],
            want: Ok(btreemap! {
                "client" => "172.17.0.1",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-03-31T12:04:07Z").unwrap().into()),
                "request" => "GET / HTTP/1.1",
                "method" => "GET",
                "path" => "/",
                "protocol" => "HTTP/1.1",
                "status" => 200,
                "size" => 612,
                "agent" => "curl/7.75.0",
            }),
            tdef: TypeDef::object(kind_combined()).fallible(),
        }

        combined_line_valid_no_compression {
            args: func_args![
                value: r#"0.0.0.0 - - [23/Apr/2021:14:59:24 +0000] "GET /my-path/manifest.json HTTP/1.1" 200 504 "https://my-url.com/my-path" "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/89.0.4389.90 Safari/537.36""#,
                format: "combined"
            ],
            want: Ok(btreemap! {
                "client" => "0.0.0.0",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-04-23T14:59:24Z").unwrap().into()),
                "request" => "GET /my-path/manifest.json HTTP/1.1",
                "method" => "GET",
                "path" => "/my-path/manifest.json",
                "protocol" => "HTTP/1.1",
                "status" => 200,
                "size" => 504,
                "referer" => "https://my-url.com/my-path",
                "agent" => "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/89.0.4389.90 Safari/537.36",
            }),
            tdef: TypeDef::object(kind_combined()).fallible(),
        }

        combined_line_valid_all_fields {
            args: func_args![
                value: r#"172.17.0.1 - alice [01/Apr/2021:12:02:31 +0000] "POST /not-found HTTP/1.1" 404 153 "http://localhost/somewhere" "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/72.0.3626.119 Safari/537.36" "2.75""#,
                format: "combined"
            ],
            want: Ok(btreemap! {
                "client" => "172.17.0.1",
                "user" => "alice",
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-04-01T12:02:31Z").unwrap().into()),
                "request" => "POST /not-found HTTP/1.1",
                "method" => "POST",
                "path" => "/not-found",
                "protocol" => "HTTP/1.1",
                "status" => 404,
                "size" => 153,
                "referer" => "http://localhost/somewhere",
                "agent" => "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/72.0.3626.119 Safari/537.36",
                "compression" => "2.75",
            }),
            tdef: TypeDef::object(kind_combined()).fallible(),
        }

        error_line_valid {
            args: func_args![
                value: r#"2021/04/01 13:02:31 [error] 31#31: *1 open() "/usr/share/nginx/html/not-found" failed (2: No such file or directory), client: 172.17.0.1, server: localhost, request: "POST /not-found HTTP/1.1", host: "localhost:8081""#,
                format: "error"
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-04-01T13:02:31Z").unwrap().into()),
                "severity" => "error",
                "pid" => 31,
                "tid" => 31,
                "cid" => 1,
                "message" => "open() \"/usr/share/nginx/html/not-found\" failed (2: No such file or directory)",
                "client" => "172.17.0.1",
                "server" => "localhost",
                "request" => "POST /not-found HTTP/1.1",
                "host" => "localhost:8081",
            }),
            tdef: TypeDef::object(kind_error()).fallible(),
        }

        error_line_with_referrer {
            args: func_args![
                value: r#"2021/06/03 09:30:50 [error] 32#32: *6 open() "/usr/share/nginx/html/favicon.ico" failed (2: No such file or directory), client: 10.244.0.0, server: localhost, request: "GET /favicon.ico HTTP/1.1", host: "65.21.190.83:31256", referrer: "http://65.21.190.83:31256/""#,
                format: "error"
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-06-03T09:30:50Z").unwrap().into()),
                "severity" => "error",
                "pid" => 32,
                "tid" => 32,
                "cid" => 6,
                "message" => "open() \"/usr/share/nginx/html/favicon.ico\" failed (2: No such file or directory)",
                "client" => "10.244.0.0",
                "server" => "localhost",
                "request" => "GET /favicon.ico HTTP/1.1",
                "host" => "65.21.190.83:31256",
                "referer" => "http://65.21.190.83:31256/",
            }),
            tdef: TypeDef::object(kind_error()).fallible(),
        }

        error_line_starting {
            args: func_args![
                value: r#"2021/06/17 19:25:59 [notice] 133309#133309: signal process started"#,
                format: "error"
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-06-17T19:25:59Z").unwrap().into()),
                "severity" => "notice",
                "pid" => 133_309,
                "tid" => 133_309,
                "message" => "signal process started",
            }),
            tdef: TypeDef::object(kind_error()).fallible(),
        }

        error_line_with_upstream {
            args: func_args![
                value: r#"2022/04/15 08:16:13 [error] 7164#7164: *20 connect() failed (113: No route to host) while connecting to upstream, client: 10.244.0.0, server: test.local, request: "GET / HTTP/2.0", upstream: "http://127.0.0.1:80/""#,
                format: "error"
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2022-04-15T08:16:13Z").unwrap().into()),
                "severity" => "error",
                "pid" => 7164,
                "tid" => 7164,
                "cid" => 20,
                "message" => "connect() failed (113: No route to host) while connecting to upstream",
                "client" => "10.244.0.0",
                "server" => "test.local",
                "request" => "GET / HTTP/2.0",
                "upstream" => "http://127.0.0.1:80/",
            }),
            tdef: TypeDef::object(kind_error()).fallible(),
        }

        error_rate_limit {
            args: func_args![
                value: r#"2022/05/30 20:56:22 [error] 7164#7164: *38068741 limiting requests, excess: 50.416 by zone "api_access_token", client: 10.244.0.0, server: test.local, request: "GET / HTTP/2.0", host: "127.0.0.1:8080""#,
                format: "error"
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2022-05-30T20:56:22Z").unwrap().into()),
                "severity" => "error",
                "pid" => 7164,
                "tid" => 7164,
                "cid" => 38_068_741,
                "message" => "limiting requests",
                "excess" => 50.416,
                "zone" => "api_access_token",
                "client" => "10.244.0.0",
                "server" => "test.local",
                "request" => "GET / HTTP/2.0",
                "host" => "127.0.0.1:8080",
            }),
            tdef: TypeDef::object(kind_error()).fallible(),
        }
    ];
}
