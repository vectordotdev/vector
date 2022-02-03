use std::collections::BTreeMap;

use vrl::prelude::*;

use crate::log_util;

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
        let variants = vec![value!("common"), value!("combined"), value!("error")];

        let value = arguments.required("value");
        let format = arguments
            .required_enum("format", &variants)?
            .try_bytes()
            .expect("format not bytes");

        let timestamp_format = arguments.optional("timestamp_format");

        Ok(Box::new(ParseApacheLogFn {
            value,
            format,
            timestamp_format,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "parse apache common log",
                source: r#"encode_json(parse_apache_log!(s'127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326', "common"))"#,
                result: Ok(
                    r#"s'{"host":"127.0.0.1","identity":"bob","message":"GET /apache_pb.gif HTTP/1.0","method":"GET","path":"/apache_pb.gif","protocol":"HTTP/1.0","size":2326,"status":200,"timestamp":"2000-10-10T20:55:36Z","user":"frank"}'"#,
                ),
            },
            Example {
                title: "parse apache combined log",
                source: r#"encode_json(parse_apache_log!(s'127.0.0.1 bob frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326 "http://www.seniorinfomediaries.com/vertical/channels/front-end/bandwidth" "Mozilla/5.0 (X11; Linux i686; rv:5.0) Gecko/1945-10-12 Firefox/37.0"', "combined"))"#,
                result: Ok(
                    r#"s'{"agent":"Mozilla/5.0 (X11; Linux i686; rv:5.0) Gecko/1945-10-12 Firefox/37.0","host":"127.0.0.1","identity":"bob","message":"GET /apache_pb.gif HTTP/1.0","method":"GET","path":"/apache_pb.gif","protocol":"HTTP/1.0","referrer":"http://www.seniorinfomediaries.com/vertical/channels/front-end/bandwidth","size":2326,"status":200,"timestamp":"2000-10-10T20:55:36Z","user":"frank"}'"#,
                ),
            },
            Example {
                title: "parse apache error log",
                source: r#"encode_json(parse_apache_log!(s'[01/Mar/2021:12:00:19 +0000] [ab:alert] [pid 4803:tid 3814] [client 147.159.108.175:24259] I will bypass the haptic COM bandwidth, that should matrix the CSS driver!', "error"))"#,
                result: Ok(
                    r#"s'{"client":"147.159.108.175","message":"I will bypass the haptic COM bandwidth, that should matrix the CSS driver!","module":"ab","pid":4803,"port":24259,"severity":"alert","thread":"3814","timestamp":"2021-03-01T12:00:19Z"}'"#,
                ),
            },
        ]
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

        let regex = match self.format.as_ref() {
            b"common" => &*log_util::REGEX_APACHE_COMMON_LOG,
            b"combined" => &*log_util::REGEX_APACHE_COMBINED_LOG,
            b"error" => &*log_util::REGEX_APACHE_ERROR_LOG,
            _ => unreachable!(),
        };

        let captures = regex
            .captures(&message)
            .ok_or("failed parsing common log line")?;

        log_util::log_fields(regex, &captures, &timestamp_format, ctx.timezone())
            .map_err(Into::into)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .fallible()
            .object(match self.format.as_ref() {
                b"common" => type_def_common(),
                b"combined" => type_def_combined(),
                b"error" => type_def_error(),
                _ => unreachable!(),
            })
    }
}

fn type_def_common() -> BTreeMap<&'static str, TypeDef> {
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

fn type_def_combined() -> BTreeMap<&'static str, TypeDef> {
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
        "referrer": Kind::Bytes | Kind::Null,
        "agent": Kind::Bytes | Kind::Null,
    }
}

fn type_def_error() -> BTreeMap<&'static str, TypeDef> {
    map! {
         "timestamp": Kind::Timestamp | Kind::Null,
         "module": Kind::Bytes | Kind::Null,
         "severity": Kind::Bytes | Kind::Null,
         "thread": Kind::Bytes | Kind::Null,
         "port": Kind::Bytes | Kind::Null,
         "message": Kind::Bytes | Kind::Null,
    }
}

#[cfg(test)]
mod tests {
    use chrono::prelude::*;
    use vector_common::btreemap;

    use super::*;

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
            tdef: TypeDef::new().fallible().object(type_def_common()),
            tz: vector_common::TimeZone::default(),
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
            tdef: TypeDef::new().fallible().object(type_def_combined()),
            tz: vector_common::TimeZone::default(),
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
            tdef: TypeDef::new().fallible().object(type_def_combined()),
            tz: vector_common::TimeZone::default(),
        }

        error_line_valid {
            args: func_args![value: r#"[01/Mar/2021:12:00:19 +0000] [ab:alert] [pid 4803:tid 3814] [client 147.159.108.175:24259] I'll bypass the haptic COM bandwidth, that should matrix the CSS driver!"#,
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
            tdef: TypeDef::new().fallible().object(type_def_error()),
            tz: vector_common::TimeZone::default(),
        }

        error_line_ip_v6 {
            args: func_args![value: r#"[01/Mar/2021:12:00:19 +0000] [ab:alert] [pid 4803:tid 3814] [client eda7:35d:3ceb:ef1e:2133:e7bf:116e:24cc:24259] I'll bypass the haptic COM bandwidth, that should matrix the CSS driver!"#,
                             format: "error"
                             ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-03-01T12:00:19Z").unwrap().into()),
                "message" => "I'll bypass the haptic COM bandwidth, that should matrix the CSS driver!",
                "module" => "ab",
                "severity" => "alert",
                "pid" => 4803,
                "thread" => "3814",
                "client" => "eda7:35d:3ceb:ef1e:2133:e7bf:116e:24cc",
                "port" => 24259
            }),
            tdef: TypeDef::new().fallible().object(type_def_error()),
            tz: vector_common::TimeZone::default(),
        }

        error_line_thread_id {
            args: func_args![
                value: r#"[2021-06-04 15:40:27.138633] [php7:emerg] [pid 4803] [client 95.223.77.60:35106] PHP Parse error:  syntax error, unexpected \'->\' (T_OBJECT_OPERATOR) in /var/www/prod/releases/master-c7225365fd9faa26262cffeeb57b31bd7448c94a/source/index.php on line 14"#,
                timestamp_format: "%Y-%m-%d %H:%M:%S.%f",
                format: "error",
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-06-04T15:40:27.000138633Z").unwrap().into()),
                "message" => "PHP Parse error:  syntax error, unexpected \\\'->\\\' (T_OBJECT_OPERATOR) in /var/www/prod/releases/master-c7225365fd9faa26262cffeeb57b31bd7448c94a/source/index.php on line 14",
                "module" => "php7",
                "severity" => "emerg",
                "pid" => 4803,
                "client" => "95.223.77.60",
                "port" => 35106

            }),
            tdef: TypeDef::new().fallible().object(type_def_error()),
            tz: vector_common::TimeZone::Named(chrono_tz::Tz::UTC),
        }

        log_line_valid_empty {
            args: func_args![value: "- - - - - - -",
                             format: "common",
            ],
            want: Ok(btreemap! {}),
            tdef: TypeDef::new().fallible().object(type_def_common()),
            tz: vector_common::TimeZone::default(),
        }

        log_line_valid_empty_variant {
            args: func_args![value: r#"- - - [-] "-" - -"#,
                             format: "common",
            ],
            want: Ok(btreemap! {}),
            tdef: TypeDef::new().fallible().object(type_def_common()),
            tz: vector_common::TimeZone::default(),
        }

        log_line_valid_with_local_timestamp_format {
            args: func_args![value: format!("[{}] - - - -",
                                            Utc.ymd(2000, 10, 10).and_hms(20,55,36)
                                              .with_timezone(&Local)
                                              .format("%a %b %d %H:%M:%S %Y")
                                            ),
                             timestamp_format: "%a %b %d %H:%M:%S %Y",
                             format: "error",
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2000-10-10T20:55:36Z").unwrap().into()),
            }),
            tdef: TypeDef::new().fallible().object(type_def_error()),
            tz: vector_common::TimeZone::default(),
        }

        log_line_valid_with_timezone {
            args: func_args![
                value: "[2021/06/03 09:30:50] - - - -",
                timestamp_format: "%Y/%m/%d %H:%M:%S",
                format: "error",
            ],
            want: Ok(btreemap! {
                "timestamp" => Value::Timestamp(DateTime::parse_from_rfc3339("2021-06-03T07:30:50Z").unwrap().into()),
            }),
            tdef: TypeDef::new().fallible().object(type_def_error()),
            tz: vector_common::TimeZone::Named(chrono_tz::Europe::Paris),
        }

        log_line_invalid {
            args: func_args![value: r#"not a common log line"#,
                             format: "common",
            ],
            want: Err("failed parsing common log line"),
            tdef: TypeDef::new().fallible().object(type_def_common()),
            tz: vector_common::TimeZone::default(),
        }

        log_line_invalid_timestamp {
            args: func_args![value: r#"- - - [1234] - - - - - "#,
                             format: "combined",
            ],
            want: Err("failed parsing timestamp 1234 using format %d/%b/%Y:%T %z: input contains invalid characters"),
            tdef: TypeDef::new().fallible().object(type_def_combined()),
            tz: vector_common::TimeZone::default(),
        }
    ];
}
