use crate::log_util;
use std::collections::BTreeMap;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct AddDatadogTags;

impl Function for AddDatadogTags {
    fn identifier(&self) -> &'static str {
        "add_datadog_tags"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "tags",
                kind: kind::ARRAY,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
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
                title: "add datadog tags",
                source: r#"encode_json(add_datadog_tags!("start:gps,end:ool", []))"#,
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

        log_util::log_fields(&regex, &captures, &timestamp_format).map_err(Into::into)
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
    use super::*;
    use chrono::prelude::*;
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
            tdef: TypeDef::new().fallible().object(type_def_common()),
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
        }


    ];
}
