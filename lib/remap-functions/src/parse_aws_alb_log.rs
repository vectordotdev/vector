use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::char,
    combinator::map_res,
    sequence::{delimited, preceded},
    IResult,
};
use remap::prelude::*;
use std::collections::BTreeMap;
use value::Kind;

#[derive(Clone, Copy, Debug)]
pub struct ParseAwsAlbLog;

impl Function for ParseAwsAlbLog {
    fn identifier(&self) -> &'static str {
        "parse_aws_alb_log"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ParseAwsAlbLogFn::new(value)))
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

impl Expression for ParseAwsAlbLogFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;

        parse_log(&String::from_utf8_lossy(&bytes))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .into_fallible(true) // Log parsing error
            .with_inner_type(inner_type_def())
            .with_constraint(value::Kind::Map)
    }
}

/// The type defs of the fields contained by the returned map.
fn inner_type_def() -> Option<InnerTypeDef> {
    Some(inner_type_def! ({
        "type": Kind::Bytes,
        "timestamp": Kind::Bytes,
        "elb": Kind::Bytes,
        "client_host": Kind::Bytes,
        "target_host": Kind::Bytes,
        "request_processing_time": Kind::Float,
        "target_processing_time": Kind::Float,
        "response_processing_time": Kind::Float,
        "elb_status_code": Kind::Bytes,
        "target_status_code": Kind::Bytes,
        "received_bytes": Kind::Integer,
        "sent_bytes": Kind::Integer,
        "request_method": Kind::Bytes,
        "request_protocol": Kind::Bytes,
        "request_url": Kind::Bytes,
        "user_agent": Kind::Bytes,
        "ssl_cipher": Kind::Bytes,
        "ssl_protocol": Kind::Bytes,
        "target_group_arn": Kind::Bytes,
        "trace_id": Kind::Bytes,
        "domain_name": Kind::Bytes,
        "chosen_cert_arn": Kind::Bytes,
        "matched_rule_priority": Kind::Bytes,
        "request_creation_time": Kind::Bytes,
        "actions_executed": Kind::Bytes,
        "redirect_url": Kind::Bytes,
        "error_reason": Kind::Bytes,
        "target_port_list": Kind::Bytes,
        "target_status_code_list": Kind::Bytes,
        "classification": Kind::Bytes,
        "classification_reason": Kind::Bytes
    }))
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
                    Value::Bytes(bytes) if bytes == &"-" => Value::Null,
                    value => value,
                },
            )
        };
    }
    macro_rules! field {
        ($name:expr, $($pattern:pat)|+) => {
            field_raw!($name, preceded(char(' '), take_while1(|c| matches!(c, $($pattern)|+))))
        };
    }
    macro_rules! field_parse {
        ($name:expr, $($pattern:pat)|+, $type:ty) => {
            field_raw!($name, map_res(preceded(char(' '), take_while1(|c| matches!(c, $($pattern)|+))), |s: &str| s.parse::<$type>()))
        };
    }

    field_raw!("type", take_while1(|c| matches!(c, 'a'..='z' | '0'..='9')));
    field!("timestamp", '0'..='9' | '.' | '-' | ':' | 'T' | 'Z');
    field_raw!("elb", take_anything);
    field!("client_host", '0'..='9' | '.' | ':' | '-');
    field!("target_host", '0'..='9' | '.' | ':' | '-');
    field_parse!("request_processing_time", '0'..='9' | '.' | '-', f64);
    field_parse!("target_processing_time", '0'..='9' | '.' | '-', f64);
    field_parse!("response_processing_time", '0'..='9' | '.' | '-', f64);
    field!("elb_status_code", '0'..='9' | '-');
    field!("target_status_code", '0'..='9' | '-');
    field_parse!("received_bytes", '0'..='9' | '-', i64);
    field_parse!("sent_bytes", '0'..='9' | '-', i64);
    let request = get_value!("request", take_quoted1);
    let mut iter = request.splitn(2, ' ');
    log.insert("request_method".to_owned(), iter.next().unwrap().into()); // split always have at least 1 item
    match iter.next() {
        Some(value) => {
            let mut iter = value.rsplitn(2, ' ');
            log.insert("request_protocol".into(), iter.next().unwrap().into()); // same as previous one
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
        take_list(|c| matches!(c, '0'..='9' | '.' | ':' | '-'))
    );
    field_raw!(
        "target_status_code_list",
        take_list(|c| matches!(c, '0'..='9'))
    );
    field_raw!("classification", take_quoted1);
    field_raw!("classification_reason", take_quoted1);

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

fn take_list(cond: impl Fn(char) -> bool) -> impl FnOnce(&str) -> SResult<Vec<&str>> {
    move |input: &str| {
        alt((
            map_res(tag(r#" "-""#), |_| {
                Ok::<_, std::convert::Infallible>(vec![])
            }),
            map_res(preceded(char(' '), take_while1(cond)), |v: &str| {
                Ok::<_, std::convert::Infallible>(vec![v])
            }),
        ))(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    remap::test_type_def![
        value_string {
            expr: |_| ParseAwsAlbLogFn { value: Literal::from("foo").boxed() },
            def: TypeDef { fallible: true, kind: value::Kind::Map, inner_type_def: inner_type_def() },
        }

        value_optional {
            expr: |_| ParseAwsAlbLogFn { value: Box::new(Noop) },
            def: TypeDef { fallible: true, kind: value::Kind::Map, inner_type_def: inner_type_def() },
        }
    ];

    #[test]
    fn parse_aws_alb_log() {
        let logs = vec![
            r#"http 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188
192.168.131.39:2817 10.0.0.1:80 0.000 0.001 0.000 200 200 34 366
"GET http://www.example.com:80/ HTTP/1.1" "curl/7.46.0" - -
arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067
"Root=1-58337262-36d228ad5d99923122bbe354" "-" "-"
0 2018-07-02T22:22:48.364000Z "forward" "-" "-" 10.0.0.1:80 200 "-" "-""#,
            r#"https 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188
192.168.131.39:2817 10.0.0.1:80 0.086 0.048 0.037 200 200 0 57
"GET https://www.example.com:443/ HTTP/1.1" "curl/7.46.0" ECDHE-RSA-AES128-GCM-SHA256 TLSv1.2
arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067
"Root=1-58337281-1d84f3d73c47ec4e58577259" "www.example.com" "arn:aws:acm:us-east-2:123456789012:certificate/12345678-1234-1234-1234-123456789012"
1 2018-07-02T22:22:48.364000Z "authenticate,forward" "-" "-" 10.0.0.1:80 200 "-" "-""#,
            r#"h2 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188
10.0.1.252:48160 10.0.0.66:9000 0.000 0.002 0.000 200 200 5 257
"GET https://10.0.2.105:773/ HTTP/2.0" "curl/7.46.0" ECDHE-RSA-AES128-GCM-SHA256 TLSv1.2
arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067
"Root=1-58337327-72bd00b0343d75b906739c42" "-" "-"
1 2018-07-02T22:22:48.364000Z "redirect" "https://example.com:80/" "-" 10.0.0.66:9000 200 "-" "-""#,
            r#"ws 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188
10.0.0.140:40914 10.0.1.192:8010 0.001 0.003 0.000 101 101 218 587
"GET http://10.0.0.30:80/ HTTP/1.1" "-" - -
arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067
"Root=1-58337364-23a8c76965a2ef7629b185e3" "-" "-"
1 2018-07-02T22:22:48.364000Z "forward" "-" "-" 10.0.1.192:8010 101 "-" "-""#,
            r#"wss 2018-07-02T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188
10.0.0.140:44244 10.0.0.171:8010 0.000 0.001 0.000 101 101 218 786
"GET https://10.0.0.30:443/ HTTP/1.1" "-" ECDHE-RSA-AES128-GCM-SHA256 TLSv1.2
arn:aws:elasticloadbalancing:us-west-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067
"Root=1-58337364-23a8c76965a2ef7629b185e3" "-" "-"
1 2018-07-02T22:22:48.364000Z "forward" "-" "-" 10.0.0.171:8010 101 "-" "-""#,
            r#"http 2018-11-30T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188
192.168.131.39:2817 - 0.000 0.001 0.000 200 200 34 366
"GET http://www.example.com:80/ HTTP/1.1" "curl/7.46.0" - -
arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067
"Root=1-58337364-23a8c76965a2ef7629b185e3" "-" "-"
0 2018-11-30T22:22:48.364000Z "forward" "-" "-" "-" "-" "-" "-""#,
            r#"http 2018-11-30T22:23:00.186641Z app/my-loadbalancer/50dc6c495c0c9188
192.168.131.39:2817 - 0.000 0.001 0.000 502 - 34 366
"GET http://www.example.com:80/ HTTP/1.1" "curl/7.46.0" - -
arn:aws:elasticloadbalancing:us-east-2:123456789012:targetgroup/my-targets/73e2d6bc24d8a067
"Root=1-58337364-23a8c76965a2ef7629b185e3" "-" "-"
0 2018-11-30T22:22:48.364000Z "forward" "-" "LambdaInvalidResponse" "-" "-" "-" "-""#,
        ];
        let logs = logs
            .into_iter()
            .map(|s| s.replace('\n', " "))
            .collect::<Vec<String>>();

        for log in logs {
            assert!(parse_log(&log).is_ok())
        }
    }
}
