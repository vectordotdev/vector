use std::collections::BTreeMap;

use ::value::Value;
use nom::{
    bytes::complete::{tag, take_while1},
    character::complete::char,
    combinator::map_res,
    sequence::{delimited, preceded},
    IResult,
};
use vrl::prelude::*;

fn parse_aws_clb_log(bytes: Value) -> Resolved {
    let bytes = bytes.try_bytes()?;
    parse_log(&String::from_utf8_lossy(&bytes))
}

#[derive(Clone, Copy, Debug)]
pub struct ParseAwsClbLog;

impl Function for ParseAwsClbLog {
    fn identifier(&self) -> &'static str {
        "parse_aws_clb_log"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: r#"parse_aws_clb_log!(s'2015-05-13T23:39:43.945958Z my-loadbalancer 192.168.131.39:2817 10.0.0.1:80 0.000073 0.001048 0.000057 200 200 0 29 "GET http://www.example.com:80/ HTTP/1.1" "curl/7.38.0" - -')"#,
            result: Ok(
                r#"{ "backend_processing_time": 0.001048, "backend_status_code": "200", "client_host": "192.168.131.39:2817", "elb": "my-loadbalancer", "elb_status_code": "200", "received_bytes": 0, "request_method": "GET", "request_processing_time": 7.3e-05, "request_protocol": "HTTP/1.1", "request_url": "http://www.example.com:80/", "response_processing_time": 5.7e-05, "sent_bytes": 29, "ssl_cipher": null, "ssl_protocol": null, "target_host": "10.0.0.1:80", "time": "2015-05-13T23:39:43.945958Z", "user_agent": "curl/7.38.0" }"#,
            ),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(ParseAwsClbLogFn::new(value).as_expr())
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
struct ParseAwsClbLogFn {
    value: Box<dyn Expression>,
}

impl ParseAwsClbLogFn {
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl FunctionExpression for ParseAwsClbLogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        parse_aws_clb_log(bytes)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(inner_kind()).fallible(/* log parsing error */)
    }
}

fn inner_kind() -> BTreeMap<Field, Kind> {
    BTreeMap::from([
        (
            Field::from("backend_processing_time"),
            Kind::bytes() | Kind::null(),
        ),
        (
            Field::from("backend_status_code"),
            Kind::bytes() | Kind::null(),
        ),
        (Field::from("client_host"), Kind::bytes()),
        (Field::from("elb"), Kind::bytes()),
        (Field::from("elb_status_code"), Kind::bytes()),
        (Field::from("received_bytes"), Kind::integer()),
        (Field::from("request_method"), Kind::bytes() | Kind::null()),
        (Field::from("request_processing_time"), Kind::float()),
        (
            Field::from("request_protocol"),
            Kind::bytes() | Kind::null(),
        ),
        (Field::from("request_url"), Kind::bytes() | Kind::null()),
        (Field::from("response_processing_time"), Kind::float()),
        (Field::from("sent_bytes"), Kind::integer()),
        (Field::from("ssl_cipher"), Kind::bytes() | Kind::null()),
        (Field::from("ssl_protocol"), Kind::bytes() | Kind::null()),
        (Field::from("target_host"), Kind::bytes() | Kind::null()),
        (Field::from("time"), Kind::bytes()),
        (Field::from("user_agent"), Kind::bytes() | Kind::null()),
    ])
}

fn parse_log(mut input: &str) -> Result<Value> {
    let mut log = BTreeMap::new();

    macro_rules! get_value {
        ($name:expr, $parser:expr) => {{
            let result: IResult<&str, _, (&str, nom::error::ErrorKind)> = $parser(input);
            match result {
                Ok((rest, value)) => {
                    input = rest;
                    value
                }
                Err(error) => {
                    return Err(format!("failed to get field `{}`: {}", $name, error).into())
                }
            }
        }};
    }
    macro_rules! field_raw {
        ($name:expr, $parser:expr) => {
            log.insert(
                $name.into(),
                match get_value!($name, $parser).into() {
                    Value::Bytes(bytes) if bytes == "-" => Value::Null,
                    value => value,
                },
            )
        };
    }
    macro_rules! field {
        ($name:expr, $($pattern:pat_param)|+) => {
            field_raw!($name, preceded(char(' '), take_while1(|c| matches!(c, $($pattern)|+))))
        };
    }
    macro_rules! field_parse {
        ($name:expr, $($pattern:pat_param)|+, $type:ty) => {
            field_raw!($name, map_res(preceded(char(' '), take_while1(|c| matches!(c, $($pattern)|+))), |s: &str| s.parse::<$type>()))
        };
    }

    field!("time", '0'..='9' | '.' | '-' | ':' | 'T' | 'Z');
    field_raw!("elb", take_anything);
    field!("client_host", '0'..='9' | 'a'..='f' | '.' | ':' | '-');
    field!("target_host", '0'..='9' | 'a'..='f' | '.' | ':' | '-');
    field_parse!(
        "request_processing_time",
        '0'..='9' | '.' | '-',
        NotNan<f64>
    );
    field_parse!(
        "backend_processing_time",
        '0'..='9' | '.' | '-',
        NotNan<f64>
    );
    field_parse!(
        "response_processing_time",
        '0'..='9' | '.' | '-',
        NotNan<f64>
    );
    field!("elb_status_code", '0'..='9' | '-');
    field!("backend_status_code", '0'..='9' | '-');
    field_parse!("received_bytes", '0'..='9' | '-', i64);
    field_parse!("sent_bytes", '0'..='9' | '-', i64);
    let request = get_value!("request", take_quoted1);
    let mut iter = request.splitn(2, ' ');
    log.insert(
        "request_method".to_owned(),
        match iter.next().unwrap().into() {
            Value::Bytes(bytes) if bytes == "-" => Value::Null,
            value => value,
        },
    ); // split always have at least 1 item
    match iter.next() {
        Some(value) => {
            let mut iter = value.rsplitn(2, ' ');
            log.insert(
                "request_protocol".to_owned(),
                match iter.next().unwrap().into() {
                    Value::Bytes(bytes) if bytes == "-" => Value::Null,
                    value => value,
                },
            ); // same as previous one
            match iter.next() {
                Some(value) => log.insert("request_url".into(), value.into()),
                None => return Err("failed to get field `request_url`".into()),
            }
        }
        None => return Err("failed to get field `request_url`".into()),
    };
    field_raw!("user_agent", take_quoted1);
    field_raw!("ssl_cipher", take_anything);
    field_raw!("ssl_protocol", take_anything);

    match input.is_empty() {
        true => Ok(log.into()),
        false => Err(format!(r#"Log should be fully consumed: "{}""#, input).into()),
    }
}

type SResult<'a, O> = IResult<&'a str, O, (&'a str, nom::error::ErrorKind)>;

fn take_anything(input: &str) -> SResult<&str> {
    preceded(char(' '), take_while1(|c| c != ' '))(input)
}

fn take_quoted1(input: &str) -> SResult<String> {
    delimited(tag(" \""), until_quote, char('"'))(input)
}

fn until_quote(input: &str) -> SResult<String> {
    let mut ret = String::new();
    let mut skip_delimiter = false;
    for (i, ch) in input.char_indices() {
        if ch == '\\' && !skip_delimiter {
            skip_delimiter = true;
        } else if ch == '"' && !skip_delimiter {
            return Ok((&input[i..], ret));
        } else {
            ret.push(ch);
            skip_delimiter = false;
        }
    }
    Err(nom::Err::Incomplete(nom::Needed::Unknown))
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_aws_clb_log => ParseAwsClbLog;

        one {
            args: func_args![value: r#"2015-05-13T23:39:43.945958Z my-loadbalancer 192.168.131.39:2817 10.0.0.1:80 0.000073 0.001048 0.000057 200 200 0 29 "GET http://www.example.com:80/ HTTP/1.1" "curl/7.38.0" - -"#],
            want: Ok(value!({backend_processing_time: 0.001048,
                             backend_status_code: "200",
                             client_host: "192.168.131.39:2817",
                             elb: "my-loadbalancer",
                             elb_status_code: "200",
                             received_bytes: 0,
                             request_method: "GET",
                             request_processing_time: 7.3e-05,
                             request_protocol: "HTTP/1.1",
                             request_url: "http://www.example.com:80/",
                             response_processing_time: 5.7e-05,
                             sent_bytes: 29,
                             ssl_cipher: null,
                             ssl_protocol: null,
                             target_host: "10.0.0.1:80",
                             time: "2015-05-13T23:39:43.945958Z",
                             user_agent: "curl/7.38.0"
            })),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        two {
            args: func_args![value: r#"2015-05-13T23:39:43.945958Z my-loadbalancer 192.168.131.39:2817 10.0.0.1:80 0.000086 0.001048 0.001337 200 200 0 57 "GET https://www.example.com:443/ HTTP/1.1" "curl/7.38.0" DHE-RSA-AES128-SHA TLSv1.2"#],
            want: Ok(value! ({backend_processing_time: 0.001048,
                              backend_status_code: "200",
                              client_host: "192.168.131.39:2817",
                              elb: "my-loadbalancer",
                              elb_status_code: "200",
                              received_bytes: 0,
                              request_method: "GET",
                              request_processing_time: 0.000086,
                              request_protocol: "HTTP/1.1",
                              request_url: "https://www.example.com:443/",
                              response_processing_time: 0.001337,
                              sent_bytes: 57,
                              ssl_cipher: "DHE-RSA-AES128-SHA",
                              ssl_protocol: "TLSv1.2",
                              target_host: "10.0.0.1:80",
                              time: "2015-05-13T23:39:43.945958Z",
                              user_agent: "curl/7.38.0"
            })),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        three {
            args: func_args![value: r#"2015-05-13T23:39:43.945958Z my-loadbalancer 192.168.131.39:2817 10.0.0.1:80 0.001069 0.000028 0.000041 - - 82 305 "- - - " "-" - -"#],
            want: Ok(value! ({backend_processing_time: 0.000028,
                              backend_status_code: null,
                              client_host: "192.168.131.39:2817",
                              elb: "my-loadbalancer",
                              elb_status_code: null,
                              received_bytes: 82,
                              request_method: null,
                              request_processing_time: 0.001069,
                              request_protocol: null,
                              request_url: null,
                              response_processing_time: 0.000041,
                              sent_bytes: 305,
                              ssl_cipher: null,
                              ssl_protocol: null,
                              target_host: "10.0.0.1:80",
                              time: "2015-05-13T23:39:43.945958Z",
                              user_agent: null
            })),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        four {
            args: func_args![value: r#"2015-05-13T23:39:43.945958Z my-loadbalancer 192.168.131.39:2817 10.0.0.1:80 0.001065 0.000015 0.000023 - - 57 502 "- - - " "-" ECDHE-ECDSA-AES128-GCM-SHA256 TLSv1.2"#],
            want: Ok(value!({backend_processing_time: 0.000015,
                             backend_status_code: null,
                             client_host: "192.168.131.39:2817",
                             elb: "my-loadbalancer",
                             elb_status_code: null,
                             received_bytes: 57,
                             request_method: null,
                             request_processing_time: 0.001065,
                             request_protocol: null,
                             request_url: null,
                             response_processing_time: 0.000023,
                             sent_bytes: 502,
                             ssl_cipher: "ECDHE-ECDSA-AES128-GCM-SHA256",
                             ssl_protocol: "TLSv1.2",
                             target_host: "10.0.0.1:80",
                             time: "2015-05-13T23:39:43.945958Z",
                             user_agent: null
            })),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }
    ];
}
