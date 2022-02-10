use std::collections::BTreeMap;

use regex::Regex;
use vrl::prelude::*;

use crate::log_util;

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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let variants = vec![value!("combined"), value!("error")];

        let value = arguments.required("value");
        let format = arguments
            .required_enum("format", &variants)?
            .try_bytes()
            .expect("format not bytes");

        let timestamp_format = arguments.optional("timestamp_format");

        Ok(Box::new(ParseNginxLogFn {
            value,
            format,
            timestamp_format,
        }))
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
        b"combined" => &*log_util::REGEX_NGINX_COMBINED_LOG,
        b"error" => &*log_util::REGEX_NGINX_ERROR_LOG,
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

impl Expression for ParseNginxLogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        let message = bytes.try_bytes_utf8_lossy()?;
        let timestamp_format = match &self.timestamp_format {
            None => time_format_for_format(self.format.as_ref()),
            Some(timestamp_format) => timestamp_format
                .resolve(ctx)?
                .try_bytes_utf8_lossy()?
                .to_string(),
        };

        let regex = regex_for_format(self.format.as_ref());

        let captures = regex.captures(&message).ok_or("failed parsing log line")?;

        log_util::log_fields(regex, &captures, &timestamp_format, ctx.timezone())
            .map(rename_referrer)
            .map_err(Into::into)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .fallible()
            .object(match self.format.as_ref() {
                b"combined" => type_def_combined(),
                b"error" => type_def_error(),
                _ => unreachable!(),
            })
    }
}

fn type_def_combined() -> BTreeMap<&'static str, TypeDef> {
    map! {
        "client": Kind::Bytes,
        "user": Kind::Bytes | Kind::Null,
        "timestamp": Kind::Timestamp,
        "request": Kind::Bytes,
        "method": Kind::Bytes,
        "path": Kind::Bytes,
        "protocol": Kind::Bytes,
        "status": Kind::Integer,
        "size": Kind::Integer,
        "referer": Kind::Bytes | Kind::Null,
        "agent": Kind::Bytes | Kind::Null,
        "compression": Kind::Bytes | Kind::Null,
    }
}

fn type_def_error() -> BTreeMap<&'static str, TypeDef> {
    map! {
        "timestamp": Kind::Timestamp,
        "severity": Kind::Bytes,
        "pid": Kind::Integer,
        "tid": Kind::Integer,
        "cid": Kind::Integer,
        "message": Kind::Bytes,
        "client": Kind::Bytes | Kind::Null,
        "server": Kind::Bytes | Kind::Null,
        "request": Kind::Bytes | Kind::Null,
        "host": Kind::Bytes | Kind::Null,
        "port": Kind::Bytes | Kind::Null,
    }
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
            tdef: TypeDef::new().fallible().object(type_def_combined()),
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
            tdef: TypeDef::new().fallible().object(type_def_combined()),
        }

        combined_line_valid_all_fields {
            args: func_args![
                value: r#"172.17.0.1 alice - [01/Apr/2021:12:02:31 +0000] "POST /not-found HTTP/1.1" 404 153 "http://localhost/somewhere" "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/72.0.3626.119 Safari/537.36" "2.75""#,
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
            tdef: TypeDef::new().fallible().object(type_def_combined()),
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
            tdef: TypeDef::new().fallible().object(type_def_error()),
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
            tdef: TypeDef::new().fallible().object(type_def_error()),
        }

        error_line_starting {
            args: func_args![
                value: r#"2021/06/17 19:25:59 [notice] 133309#133309: signal process started"#,
                format: "error"
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-06-17T19:25:59Z").unwrap().into()),
                "severity" => "notice",
                "pid" => 133309,
                "tid" => 133309,
                "message" => "signal process started",
            }),
            tdef: TypeDef::new().fallible().object(type_def_error()),
        }
    ];
}
