use std::collections::BTreeMap;

use ::value::Value;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::char,
    combinator::map_res,
    sequence::{delimited, preceded},
    IResult,
};
use vrl::prelude::*;

fn parse_aws_alb_log(bytes: Value) -> Resolved {
    let bytes = bytes.try_bytes()?;
    parse_log(&String::from_utf8_lossy(&bytes))
}

#[derive(Clone, Copy, Debug)]
pub struct ParseAwsAlbLog;

impl Function for ParseAwsAlbLog {
    fn identifier(&self) -> &'static str {
        "parse_aws_alb_log"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            source: r#"parse_aws_alb_log!(s'http 2018-11-30T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 192.168.131.39:2817 - 0.000 0.001 0.000 200 200 34 366 "GET http://www.example.com:80/ HTTP/1.1" "curl/7.46.0" - - arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 "Root=1-58337364-23a8c76965a2ef7629b185e3" "-" "-" 0 2018-11-30T22:22:48.364000Z "forward" "-" "-" "-" "-" "-" "-"')"#,
            result: Ok(
                r#"{ "actions_executed": "forward", "chosen_cert_arn": null, "classification": null, "classification_reason": null, "client_host": "192.168.131.39:2817", "domain_name": null, "elb": "app/my-loadbalancer/50dc6c495c0c9188", "elb_status_code": "200", "error_reason": null, "matched_rule_priority": "0", "received_bytes": 34, "redirect_url": null, "request_creation_time": "2018-11-30T22:22:48.364000Z", "request_method": "GET", "request_processing_time": 0.0, "request_protocol": "HTTP/1.1", "request_url": "http://www.example.com:80/", "response_processing_time": 0.0, "sent_bytes": 366, "ssl_cipher": null, "ssl_protocol": null, "target_group_arn": "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067", "target_host": null, "target_port_list": [], "target_processing_time": 0.001, "target_status_code": "200", "target_status_code_list": [], "timestamp": "2018-11-30T22:23:00.186641Z", "trace_id": "Root=1-58337364-23a8c76965a2ef7629b185e3", "type": "http", "user_agent": "curl/7.46.0" }"#,
            ),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(ParseAwsAlbLogFn::new(value).as_expr())
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
struct ParseAwsAlbLogFn {
    value: Box<dyn Expression>,
}

impl ParseAwsAlbLogFn {
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl FunctionExpression for ParseAwsAlbLogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        parse_aws_alb_log(bytes)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::object(inner_kind()).fallible(/* log parsing error */)
    }
}

fn inner_kind() -> BTreeMap<Field, Kind> {
    BTreeMap::from([
        (
            Field::from("actions_executed"),
            Kind::bytes() | Kind::null(),
        ),
        (Field::from("chosen_cert_arn"), Kind::bytes() | Kind::null()),
        (
            Field::from("classification_reason"),
            Kind::bytes() | Kind::null(),
        ),
        (Field::from("classification"), Kind::bytes() | Kind::null()),
        (Field::from("client_host"), Kind::bytes()),
        (Field::from("domain_name"), Kind::bytes() | Kind::null()),
        (Field::from("elb_status_code"), Kind::bytes()),
        (Field::from("elb"), Kind::bytes()),
        (Field::from("error_reason"), Kind::bytes() | Kind::null()),
        (
            Field::from("matched_rule_priority"),
            Kind::bytes() | Kind::null(),
        ),
        (Field::from("received_bytes"), Kind::integer()),
        (Field::from("redirect_url"), Kind::bytes() | Kind::null()),
        (Field::from("request_creation_time"), Kind::bytes()),
        (Field::from("request_method"), Kind::bytes()),
        (Field::from("request_processing_time"), Kind::float()),
        (Field::from("request_protocol"), Kind::bytes()),
        (Field::from("request_url"), Kind::bytes()),
        (Field::from("response_processing_time"), Kind::float()),
        (Field::from("sent_bytes"), Kind::integer()),
        (Field::from("ssl_cipher"), Kind::bytes() | Kind::null()),
        (Field::from("ssl_protocol"), Kind::bytes() | Kind::null()),
        (Field::from("target_group_arn"), Kind::bytes()),
        (Field::from("target_host"), Kind::bytes() | Kind::null()),
        (
            Field::from("target_port_list"),
            Kind::bytes() | Kind::null(),
        ),
        (Field::from("target_processing_time"), Kind::float()),
        (
            Field::from("target_status_code_list"),
            Kind::bytes() | Kind::null(),
        ),
        (
            Field::from("target_status_code"),
            Kind::bytes() | Kind::null(),
        ),
        (Field::from("timestamp"), Kind::bytes()),
        (Field::from("trace_id"), Kind::bytes()),
        (Field::from("type"), Kind::bytes()),
        (Field::from("user_agent"), Kind::bytes()),
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

    field_raw!("type", take_while1(|c| matches!(c, 'a'..='z' | '0'..='9')));
    field!("timestamp", '0'..='9' | '.' | '-' | ':' | 'T' | 'Z');
    field_raw!("elb", take_anything);
    field!("client_host", '0'..='9' | 'a'..='f' | '.' | ':' | '-');
    field!("target_host", '0'..='9' | 'a'..='f' | '.' | ':' | '-');
    field_parse!(
        "request_processing_time",
        '0'..='9' | '.' | '-',
        NotNan<f64>
    );
    field_parse!("target_processing_time", '0'..='9' | '.' | '-', NotNan<f64>);
    field_parse!(
        "response_processing_time",
        '0'..='9' | '.' | '-',
        NotNan<f64>
    );
    field!("elb_status_code", '0'..='9' | '-');
    field!("target_status_code", '0'..='9' | '-');
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
    field_raw!("target_group_arn", take_anything);
    field_raw!("trace_id", take_quoted1);
    field_raw!("domain_name", take_quoted1);
    field_raw!("chosen_cert_arn", take_quoted1);
    field!("matched_rule_priority", '0'..='9' | '-');
    field!(
        "request_creation_time",
        '0'..='9' | '.' | '-' | ':' | 'T' | 'Z'
    );
    field_raw!("actions_executed", take_quoted1);
    field_raw!("redirect_url", take_quoted1);
    field_raw!("error_reason", take_quoted1);
    field_raw!(
        "target_port_list",
        take_maybe_quoted_list(|c| matches!(c, '0'..='9' | 'a'..='f' | '.' | ':' | '-'))
    );
    field_raw!(
        "target_status_code_list",
        take_maybe_quoted_list(|c| matches!(c, '0'..='9'))
    );
    field_raw!("classification", take_quoted1);
    field_raw!("classification_reason", take_quoted1);

    match input.is_empty() {
        true => Ok(log.into()),
        false => Err(format!(r#"Log should be fully consumed: "{input}""#).into()),
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

fn take_maybe_quoted_list<'a>(
    cond: impl Fn(char) -> bool + Clone,
) -> impl FnOnce(&'a str) -> SResult<Vec<&'a str>> {
    alt((
        map_res(tag(r#" "-""#), |_| {
            Ok::<_, std::convert::Infallible>(vec![])
        }),
        map_res(
            delimited(tag(" \""), take_while1(cond.clone()), char('"')),
            |v: &str| Ok::<_, std::convert::Infallible>(vec![v]),
        ),
        map_res(preceded(char(' '), take_while1(cond)), |v: &str| {
            Ok::<_, std::convert::Infallible>(vec![v])
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_aws_alb_log => ParseAwsAlbLog;

        one {
            args: func_args![value: r#"http 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 192.168.131.39:2817 10.0.0.1:80 0.000 0.001 0.000 200 200 34 366 "GET http://www.example.com:80/ HTTP/1.1" "curl/7.46.0" - - arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 "Root=1-58337262-36d228ad5d99923122bbe354" "-" "-" 0 2018-07-02T22:22:48.364000Z "forward" "-" "-" 10.0.0.1:80 200 "-" "-""#],
            want: Ok(value!({actions_executed: "forward",
                             chosen_cert_arn: null,
                             classification: null,
                             classification_reason: null,
                             client_host: "192.168.131.39:2817",
                             domain_name: null,
                             elb: "app/my-loadbalancer/50dc6c495c0c9188",
                             elb_status_code: "200",
                             error_reason: null,
                             matched_rule_priority: "0",
                             received_bytes: 34,
                             redirect_url: null,
                             request_creation_time: "2018-07-02T22:22:48.364000Z",
                             request_method: "GET",
                             request_processing_time: 0.0,
                             request_protocol: "HTTP/1.1",
                             request_url: "http://www.example.com:80/",
                             response_processing_time: 0.0,
                             sent_bytes: 366,
                             ssl_cipher: null,
                             ssl_protocol: null,
                             target_group_arn: "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067",
                             target_host: "10.0.0.1:80",
                             target_port_list: ["10.0.0.1:80"],
                             target_processing_time: 0.001,
                             target_status_code: "200",
                             target_status_code_list: ["200"],
                             timestamp: "2018-07-02T22:23:00.186641Z",
                             trace_id: "Root=1-58337262-36d228ad5d99923122bbe354",
                             type: "http",
                             user_agent: "curl/7.46.0"

            })),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        two {
            args: func_args![value: r#"https 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 192.168.131.39:2817 10.0.0.1:80 0.086 0.048 0.037 200 200 0 57 "GET https://www.example.com:443/ HTTP/1.1" "curl/7.46.0" ECDHE-RSA-AES128-GCM-SHA256 TLSv1.2 arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 "Root=1-58337281-1d84f3d73c47ec4e58577259" "www.example.com" "arn:aws:acm:us-east-2:123456789012:certificate/12345678-1234-1234-1234-123456789012" 1 2018-07-02T22:22:48.364000Z "authenticate,forward" "-" "-" 10.0.0.1:80 200 "-" "-""#],
            want: Ok(value! ({actions_executed: "authenticate,forward",
                              chosen_cert_arn: "arn:aws:acm:us-east-2:123456789012:certificate/12345678-1234-1234-1234-123456789012",
                              classification: null,
                              classification_reason: null,
                              client_host: "192.168.131.39:2817",
                              domain_name: "www.example.com",
                              elb: "app/my-loadbalancer/50dc6c495c0c9188",
                              elb_status_code: "200",
                              error_reason: null,
                              matched_rule_priority: "1",
                              received_bytes: 0,
                              redirect_url: null,
                              request_creation_time: "2018-07-02T22:22:48.364000Z",
                              request_method: "GET",
                              request_processing_time: 0.086,
                              request_protocol: "HTTP/1.1",
                              request_url: "https://www.example.com:443/",
                              response_processing_time: 0.037,
                              sent_bytes: 57,
                              ssl_cipher: "ECDHE-RSA-AES128-GCM-SHA256",
                              ssl_protocol: "TLSv1.2",
                              target_group_arn: "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067",
                              target_host: "10.0.0.1:80",
                              target_port_list: ["10.0.0.1:80"],
                              target_processing_time: 0.048,
                              target_status_code: "200",
                              target_status_code_list: ["200"],
                              timestamp: "2018-07-02T22:23:00.186641Z",
                              trace_id: "Root=1-58337281-1d84f3d73c47ec4e58577259",
                              type: "https",
                              user_agent: "curl/7.46.0"})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        three {
            args: func_args![value: r#"h2 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 10.0.1.252:48160 10.0.0.66:9000 0.000 0.002 0.000 200 200 5 257 "GET https://10.0.2.105:773/ HTTP/2.0" "curl/7.46.0" ECDHE-RSA-AES128-GCM-SHA256 TLSv1.2 arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 "Root=1-58337327-72bd00b0343d75b906739c42" "-" "-" 1 2018-07-02T22:22:48.364000Z "redirect" "https://example.com:80/" "-" 10.0.0.66:9000 200 "-" "-""#],
            want: Ok(value! ({actions_executed: "redirect",
                              chosen_cert_arn: null,
                              classification: null,
                              classification_reason: null,
                              client_host: "10.0.1.252:48160",
                              domain_name: null,
                              elb: "app/my-loadbalancer/50dc6c495c0c9188",
                              elb_status_code: "200",
                              error_reason: null,
                              matched_rule_priority: "1",
                              received_bytes: 5,
                              redirect_url: "https://example.com:80/",
                              request_creation_time: "2018-07-02T22:22:48.364000Z",
                              request_method: "GET",
                              request_processing_time: 0.0,
                              request_protocol: "HTTP/2.0",
                              request_url: "https://10.0.2.105:773/",
                              response_processing_time: 0.0,
                              sent_bytes: 257,
                              ssl_cipher: "ECDHE-RSA-AES128-GCM-SHA256",
                              ssl_protocol: "TLSv1.2",
                              target_group_arn: "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067",
                              target_host: "10.0.0.66:9000",
                              target_port_list: ["10.0.0.66:9000"],
                              target_processing_time: 0.002,
                              target_status_code: "200",
                              target_status_code_list: ["200"],
                              timestamp: "2018-07-02T22:23:00.186641Z",
                              trace_id: "Root=1-58337327-72bd00b0343d75b906739c42",
                              type: "h2",
                              user_agent: "curl/7.46.0"})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        four {
            args: func_args![value: r#"ws 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 10.0.0.140:40914 10.0.1.192:8010 0.001 0.003 0.000 101 101 218 587 "GET http://10.0.0.30:80/ HTTP/1.1" "-" - - arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 "Root=1-58337364-23a8c76965a2ef7629b185e3" "-" "-" 1 2018-07-02T22:22:48.364000Z "forward" "-" "-" 10.0.1.192:8010 101 "-" "-""#],
            want: Ok(value!({actions_executed: "forward",
                             chosen_cert_arn: null,
                             classification: null,
                             classification_reason: null,
                             client_host: "10.0.0.140:40914",
                             domain_name: null,
                             elb: "app/my-loadbalancer/50dc6c495c0c9188",
                             elb_status_code: "101",
                             error_reason: null,
                             matched_rule_priority: "1",
                             received_bytes: 218,
                             redirect_url: null,
                             request_creation_time: "2018-07-02T22:22:48.364000Z",
                             request_method: "GET",
                             request_processing_time: 0.001,
                             request_protocol: "HTTP/1.1",
                             request_url: "http://10.0.0.30:80/",
                             response_processing_time: 0.0,
                             sent_bytes: 587,
                             ssl_cipher: null,
                             ssl_protocol: null,
                             target_group_arn: "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067",
                             target_host: "10.0.1.192:8010",
                             target_port_list: ["10.0.1.192:8010"],
                             target_processing_time: 0.003,
                             target_status_code: "101",
                             target_status_code_list: ["101"],
                             timestamp: "2018-07-02T22:23:00.186641Z",
                             trace_id: "Root=1-58337364-23a8c76965a2ef7629b185e3",
                             type: "ws",
                             user_agent: null})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        five {
            args: func_args![value: r#"wss 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 10.0.0.140:44244 10.0.0.171:8010 0.000 0.001 0.000 101 101 218 786 "GET https://10.0.0.30:443/ HTTP/1.1" "-" ECDHE-RSA-AES128-GCM-SHA256 TLSv1.2 arn:aws:elasticloadbalancing:us-west-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 "Root=1-58337364-23a8c76965a2ef7629b185e3" "-" "-" 1 2018-07-02T22:22:48.364000Z "forward" "-" "-" 10.0.0.171:8010 101 "-" "-""#],
            want: Ok(value! ({actions_executed: "forward",
                              chosen_cert_arn: null,
                              classification: null,
                              classification_reason: null,
                              client_host: "10.0.0.140:44244",
                              domain_name: null,
                              elb: "app/my-loadbalancer/50dc6c495c0c9188",
                              elb_status_code: "101",
                              error_reason: null,
                              matched_rule_priority: "1",
                              received_bytes: 218,
                              redirect_url: null,
                              request_creation_time: "2018-07-02T22:22:48.364000Z",
                              request_method: "GET",
                              request_processing_time: 0.0,
                              request_protocol: "HTTP/1.1",
                              request_url: "https://10.0.0.30:443/",
                              response_processing_time: 0.0,
                              sent_bytes: 786,
                              ssl_cipher: "ECDHE-RSA-AES128-GCM-SHA256",
                              ssl_protocol: "TLSv1.2",
                              target_group_arn: "arn:aws:elasticloadbalancing:us-west-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067",
                              target_host: "10.0.0.171:8010",
                              target_port_list: ["10.0.0.171:8010"],
                              target_processing_time: 0.001,
                              target_status_code: "101",
                              target_status_code_list: ["101"],
                              timestamp: "2018-07-02T22:23:00.186641Z",
                              trace_id: "Root=1-58337364-23a8c76965a2ef7629b185e3",
                              type: "wss",
                              user_agent: null})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        six {
            args: func_args![value: r#"http 2018-11-30T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 192.168.131.39:2817 - 0.000 0.001 0.000 200 200 34 366 "GET http://www.example.com:80/ HTTP/1.1" "curl/7.46.0" - - arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 "Root=1-58337364-23a8c76965a2ef7629b185e3" "-" "-" 0 2018-11-30T22:22:48.364000Z "forward" "-" "-" "-" "-" "-" "-""#],
            want: Ok(value! ({actions_executed: "forward",
                              chosen_cert_arn: null,
                              classification: null,
                              classification_reason: null,
                              client_host: "192.168.131.39:2817",
                              domain_name: null,
                              elb: "app/my-loadbalancer/50dc6c495c0c9188",
                              elb_status_code: "200",
                              error_reason: null,
                              matched_rule_priority: "0",
                              received_bytes: 34,
                              redirect_url: null,
                              request_creation_time: "2018-11-30T22:22:48.364000Z",
                              request_method: "GET",
                              request_processing_time: 0.0,
                              request_protocol: "HTTP/1.1",
                              request_url: "http://www.example.com:80/",
                              response_processing_time: 0.0,
                              sent_bytes: 366,
                              ssl_cipher: null,
                              ssl_protocol: null,
                              target_group_arn: "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067",
                              target_host: null,
                              target_port_list: [],
                              target_processing_time: 0.001,
                              target_status_code: "200",
                              target_status_code_list: [],
                              timestamp: "2018-11-30T22:23:00.186641Z",
                              trace_id: "Root=1-58337364-23a8c76965a2ef7629b185e3",
                              type: "http",
                              user_agent: "curl/7.46.0"})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        seven {
            args: func_args![value: r#"http 2018-11-30T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188 192.168.131.39:2817 - 0.000 0.001 0.000 502 - 34 366 "GET http://www.example.com:80/ HTTP/1.1" "curl/7.46.0" - - arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067 "Root=1-58337364-23a8c76965a2ef7629b185e3" "-" "-" 0 2018-11-30T22:22:48.364000Z "forward" "-" "LambdaInvalidResponse" "-" "-" "-" "-""#],
            want: Ok(value!({actions_executed: "forward",
                             chosen_cert_arn: null,
                             classification: null,
                             classification_reason: null,
                             client_host: "192.168.131.39:2817",
                             domain_name: null,
                             elb: "app/my-loadbalancer/50dc6c495c0c9188",
                             elb_status_code: "502",
                             error_reason: "LambdaInvalidResponse",
                             matched_rule_priority: "0",
                             received_bytes: 34,
                             redirect_url: null,
                             request_creation_time: "2018-11-30T22:22:48.364000Z",
                             request_method: "GET",
                             request_processing_time: 0.0,
                             request_protocol: "HTTP/1.1",
                             request_url: "http://www.example.com:80/",
                             response_processing_time: 0.0,
                             sent_bytes: 366,
                             ssl_cipher: null,
                             ssl_protocol: null,
                             target_group_arn: "arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067",
                             target_host: null,
                             target_port_list: [],
                             target_processing_time: 0.001,
                             target_status_code: null,
                             target_status_code_list: [],
                             timestamp: "2018-11-30T22:23:00.186641Z",
                             trace_id: "Root=1-58337364-23a8c76965a2ef7629b185e3",
                             type: "http",
                             user_agent: "curl/7.46.0"})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        eight {
            args: func_args![value: r#"https 2021-03-16T20:20:00.135052Z app/awseb-AWSEB-1MVD8OW91UMOH/a32a5528b8fdaa6b 10.209.14.140:50599 10.119.5.47:80 0.001 0.052 0.000 200 200 589 2084 "POST https://test.domain.com:443/api/deposits/transactions:search?detailsLevel=FULL&offset=0&limit=50 HTTP/1.1" "User 1.0" ECDHE-RSA-AES128-GCM-SHA256 TLSv1.2 arn:aws:elasticloadbalancing:us-east-1:755269215481:targetgroup/awseb-AWSEB-91MZX0WA1A0F/5a03cc723870f039 "Root=1-605112f0-31f367be4fd3da651daa4157" "some.domain.com" "arn:aws:acm:us-east-1:765229915481:certificate/d8450a8a-b4f6-4714-8535-17c625c36899" 0 2021-03-16T20:20:00.081000Z "waf,forward" "-" "-" "10.229.5.47:80" "200" "-" "-""#],
            want: Ok(value!({type: "https",
                             timestamp: "2021-03-16T20:20:00.135052Z",
                             elb: "app/awseb-AWSEB-1MVD8OW91UMOH/a32a5528b8fdaa6b",
                             client_host: "10.209.14.140:50599",
                             target_host: "10.119.5.47:80",
                             request_processing_time: 0.001,
                             target_processing_time: 0.052,
                             response_processing_time: 0.0,
                             elb_status_code: "200",
                             target_status_code: "200",
                             received_bytes: 589,
                             sent_bytes: 2084,
                             request_method: "POST",
                             request_url: "https://test.domain.com:443/api/deposits/transactions:search?detailsLevel=FULL&offset=0&limit=50",
                             request_protocol: "HTTP/1.1",
                             user_agent: "User 1.0",
                             ssl_cipher: "ECDHE-RSA-AES128-GCM-SHA256",
                             ssl_protocol: "TLSv1.2",
                             target_group_arn: "arn:aws:elasticloadbalancing:us-east-1:755269215481:targetgroup/awseb-AWSEB-91MZX0WA1A0F/5a03cc723870f039",
                             trace_id: "Root=1-605112f0-31f367be4fd3da651daa4157",
                             domain_name: "some.domain.com",
                             chosen_cert_arn: "arn:aws:acm:us-east-1:765229915481:certificate/d8450a8a-b4f6-4714-8535-17c625c36899",
                             matched_rule_priority: "0",
                             request_creation_time: "2021-03-16T20:20:00.081000Z",
                             actions_executed: "waf,forward",
                             redirect_url: null,
                             error_reason: null,
                             target_port_list: ["10.229.5.47:80"],
                             target_status_code_list: ["200"],
                             classification: null,
                             classification_reason: null})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        nine {
            args: func_args![value: r#"http 2021-03-17T18:54:14.216357Z app/awseb-AWSEB-1KGU6EBAG3FAD/7f0dc2b05788640f 113.241.19.90:15070 - -1 -1 -1 400 - 0 272 "- http://awseb-awseb-1kgu6fbag3fad-640112591.us-east-1.elb.amazonaws.com:80- -" "-" - - - "-" "-" "-" - 2021-03-17T18:54:13.967000Z "-" "-" "-" "-" "-" "-" "-""#],
            want: Ok(value!({type: "http",
                             timestamp: "2021-03-17T18:54:14.216357Z",
                             elb: "app/awseb-AWSEB-1KGU6EBAG3FAD/7f0dc2b05788640f",
                             client_host: "113.241.19.90:15070",
                             target_host: null,
                             request_processing_time: (-1.0),
                             target_processing_time: (-1.0),
                             response_processing_time: (-1.0),
                             elb_status_code: "400",
                             target_status_code: null,
                             received_bytes: 0,
                             sent_bytes: 272,
                             request_method: null,
                             request_url: "http://awseb-awseb-1kgu6fbag3fad-640112591.us-east-1.elb.amazonaws.com:80-",
                             request_protocol: null,
                             user_agent: null,
                             ssl_cipher: null,
                             ssl_protocol: null,
                             target_group_arn: null,
                             trace_id: null,
                             domain_name: null,
                             chosen_cert_arn: null,
                             matched_rule_priority: null,
                             request_creation_time: "2021-03-17T18:54:13.967000Z",
                             actions_executed: null,
                             redirect_url: null,
                             error_reason: null,
                             target_port_list: [],
                             target_status_code_list: [],
                             classification: null,
                             classification_reason: null})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        ten {
            args: func_args![value: r#"http 2021-03-18T04:00:26.920977Z app/awseb-AWSEB-1KGU6EBAG3FAD/7f0dc2b05788640f 31.211.20.175:57720 - -1 -1 -1 400 - 191 272 "POST http://awseb-awseb-1kgu6fbag3fad-640112591.us-east-1.elb.amazonaws.com:80/cgi-bin/login.cgi HTTP/1.1" "Mozilla/5.0 (X11; Linux x86_64; rv:60.0) Gecko/20100101 Firefox/60.0" - - - "-" "-" "-" - 2021-03-18T04:00:26.599000Z "-" "-" "-" "-" "-" "-" "-""#],
            want: Ok(value!({type: "http",
                             timestamp: "2021-03-18T04:00:26.920977Z",
                             elb: "app/awseb-AWSEB-1KGU6EBAG3FAD/7f0dc2b05788640f",
                             client_host: "31.211.20.175:57720",
                             target_host: null,
                             request_processing_time: (-1.0),
                             target_processing_time: (-1.0),
                             response_processing_time: (-1.0),
                             elb_status_code: "400",
                             target_status_code: null,
                             received_bytes: 191,
                             sent_bytes: 272,
                             request_method: "POST",
                             request_url: "http://awseb-awseb-1kgu6fbag3fad-640112591.us-east-1.elb.amazonaws.com:80/cgi-bin/login.cgi",
                             request_protocol: "HTTP/1.1",
                             user_agent: "Mozilla/5.0 (X11; Linux x86_64; rv:60.0) Gecko/20100101 Firefox/60.0",
                             ssl_cipher: null,
                             ssl_protocol: null,
                             target_group_arn: null,
                             trace_id: null,
                             domain_name: null,
                             chosen_cert_arn: null,
                             matched_rule_priority: null,
                             request_creation_time: "2021-03-18T04:00:26.599000Z",
                             actions_executed: null,
                             redirect_url: null,
                             error_reason: null,
                             target_port_list: [],
                             target_status_code_list: [],
                             classification: null,
                             classification_reason: null})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }

        eleven {
            args: func_args![value: r#"https 2021-03-16T20:20:00.135052Z app/awseb-AWSEB-1MVD8OW91UMOH/a32a5528b8fdaa6b 2601:6bbc:c529:9dad:6bbc:c529:9dad:6bbc:50599 fd6d:6bbc:c529:6::face:ed83:f46:80 0.001 0.052 0.000 200 200 589 2084 "POST https://test.domain.com:443/api/deposits/transactions:search?detailsLevel=FULL&offset=0&limit=50 HTTP/1.1" "User 1.0" ECDHE-RSA-AES128-GCM-SHA256 TLSv1.2 arn:aws:elasticloadbalancing:us-east-1:755269215481:targetgroup/awseb-AWSEB-91MZX0WA1A0F/5a03cc723870f039 "Root=1-605112f0-31f367be4fd3da651daa4157" "some.domain.com" "arn:aws:acm:us-east-1:765229915481:certificate/d8450a8a-b4f6-4714-8535-17c625c36899" 0 2021-03-16T20:20:00.081000Z "waf,forward" "-" "-" "fd6d:6bbc:c529:27ff:b::dead:ed84:80" "200" "-" "-""#],
            want: Ok(value!({type: "https",
                             timestamp: "2021-03-16T20:20:00.135052Z",
                             elb: "app/awseb-AWSEB-1MVD8OW91UMOH/a32a5528b8fdaa6b",
                             client_host: "2601:6bbc:c529:9dad:6bbc:c529:9dad:6bbc:50599",
                             target_host: "fd6d:6bbc:c529:6::face:ed83:f46:80",
                             request_processing_time: 0.001,
                             target_processing_time: 0.052,
                             response_processing_time: 0.0,
                             elb_status_code: "200",
                             target_status_code: "200",
                             received_bytes: 589,
                             sent_bytes: 2084,
                             request_method: "POST",
                             request_url: "https://test.domain.com:443/api/deposits/transactions:search?detailsLevel=FULL&offset=0&limit=50",
                             request_protocol: "HTTP/1.1",
                             user_agent: "User 1.0",
                             ssl_cipher: "ECDHE-RSA-AES128-GCM-SHA256",
                             ssl_protocol: "TLSv1.2",
                             target_group_arn: "arn:aws:elasticloadbalancing:us-east-1:755269215481:targetgroup/awseb-AWSEB-91MZX0WA1A0F/5a03cc723870f039",
                             trace_id: "Root=1-605112f0-31f367be4fd3da651daa4157",
                             domain_name: "some.domain.com",
                             chosen_cert_arn: "arn:aws:acm:us-east-1:765229915481:certificate/d8450a8a-b4f6-4714-8535-17c625c36899",
                             matched_rule_priority: "0",
                             request_creation_time: "2021-03-16T20:20:00.081000Z",
                             actions_executed: "waf,forward",
                             redirect_url: null,
                             error_reason: null,
                             target_port_list: ["fd6d:6bbc:c529:27ff:b::dead:ed84:80"],
                             target_status_code_list: ["200"],
                             classification: null,
                             classification_reason: null})),
            tdef: TypeDef::object(inner_kind()).fallible(),
        }
    ];
}
