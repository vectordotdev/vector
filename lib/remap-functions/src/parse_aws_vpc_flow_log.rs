use remap::prelude::*;
use std::collections::BTreeMap;
use value::Kind;

#[derive(Clone, Copy, Debug)]
pub struct ParseAwsVpcFlowLog;

impl Function for ParseAwsVpcFlowLog {
    fn identifier(&self) -> &'static str {
        "parse_aws_vpc_flow_log"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "format",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let format = arguments.optional("format").map(Expr::boxed);

        Ok(Box::new(ParseAwsVpcFlowLogFn::new(value, format)))
    }
}

#[derive(Debug, Clone)]
struct ParseAwsVpcFlowLogFn {
    value: Box<dyn Expression>,
    format: Option<Box<dyn Expression>>,
}

impl ParseAwsVpcFlowLogFn {
    fn new(value: Box<dyn Expression>, format: Option<Box<dyn Expression>>) -> Self {
        Self { value, format }
    }
}

impl Expression for ParseAwsVpcFlowLogFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let input = String::from_utf8_lossy(&bytes);

        match &self.format {
            Some(expr) => {
                let bytes = expr.execute(state, object)?.try_bytes()?;
                parse_log(&input, Some(&String::from_utf8_lossy(&bytes)))
            }
            None => parse_log(&input, None),
        }
        .map_err(Into::into)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .into_fallible(true) // Log parsin_ error
            .with_inner_type(inner_type_def())
            .with_constraint(value::Kind::Map)
    }
}

/// The type defs of the fields contained by the returned map.
fn inner_type_def() -> Option<InnerTypeDef> {
    Some(inner_type_def! ({
        "version": Kind::Integer | Kind::Null,
        "account_id": Kind::Integer | Kind::Null,
        "interface_id": Kind::Bytes | Kind::Null,
        "srcaddr": Kind::Bytes | Kind::Null,
        "dstaddr": Kind::Bytes | Kind::Null,
        "srcport": Kind::Integer | Kind::Null,
        "dstport": Kind::Integer | Kind::Null,
        "protocol": Kind::Integer | Kind::Null,
        "packets": Kind::Integer | Kind::Null,
        "bytes": Kind::Integer | Kind::Null,
        "start": Kind::Integer | Kind::Null,
        "end": Kind::Integer | Kind::Null,
        "action": Kind::Bytes | Kind::Null,
        "log_status": Kind::Bytes | Kind::Null,
        "vpc_id": Kind::Bytes | Kind::Null,
        "subnet_id": Kind::Bytes | Kind::Null,
        "instance_id": Kind::Bytes | Kind::Null,
        "tcp_flags": Kind::Integer | Kind::Null,
        "type": Kind::Bytes | Kind::Null,
        "pkt_srcaddr": Kind::Bytes | Kind::Null,
        "pkt_dstaddr": Kind::Bytes | Kind::Null,
        "region": Kind::Bytes | Kind::Null,
        "az_id": Kind::Bytes | Kind::Null,
        "sublocation_type": Kind::Bytes | Kind::Null,
    }))
}

type ParseResult<T> = std::result::Result<T, String>;

fn identity<'a>(_key: &'a str, value: &'a str) -> ParseResult<&'a str> {
    Ok(value)
}

fn parse_i64(key: &str, value: &str) -> ParseResult<i64> {
    value
        .parse()
        .map_err(|_| format!("failed to parse value as i64 (key: `{}`): `{}`", key, value))
}

macro_rules! create_match {
    ($log:expr, $key:expr, $value:expr, $($name:expr => $transform:expr),+) => {
        match $key {
            $($name => {
                let value = match $value {
                    "-" => Value::Null,
                    value => $transform($name, value)?.into(),
                };
                if $log.insert($name.into(), value).is_some() {
                    return Err(format!("value already exists for key: `{}`", $key));
                }
            })+
            key => return Err(format!("unknown key: `{}`", key))
        };
    };
}

fn parse_log(input: &str, format: Option<&str>) -> ParseResult<Value> {
    let mut log = BTreeMap::new();

    let mut input = input.split(' ');
    let mut format = format
        .unwrap_or("version account_id interface_id srcaddr dstaddr srcport dstport protocol packets bytes start end action log_status")
        .split(' ');

    loop {
        return match (format.next(), input.next()) {
            (Some(key), Some(value)) => {
                create_match!(
                    log, key, value,
                    "version" => parse_i64,
                    "account_id" => parse_i64,
                    "interface_id" => identity,
                    "srcaddr" => identity,
                    "dstaddr" => identity,
                    "srcport" => parse_i64,
                    "dstport" => parse_i64,
                    "protocol" => parse_i64,
                    "packets" => parse_i64,
                    "bytes" => parse_i64,
                    "start" => parse_i64,
                    "end" => parse_i64,
                    "action" => identity,
                    "log_status" => identity,
                    "vpc_id" => identity,
                    "subnet_id" => identity,
                    "instance_id" => identity,
                    "tcp_flags" => parse_i64,
                    "type" => identity,
                    "pkt_srcaddr" => identity,
                    "pkt_dstaddr" => identity,
                    "region" => identity,
                    "az_id" => identity,
                    "sublocation_type" => identity,
                    "sublocation_id" => identity
                );

                continue;
            }
            (None, Some(value)) => Err(format!("no key for value: `{}`", value)),
            (Some(key), None) => Err(format!("no item for key: `{}`", key)),
            (None, None) => Ok(log.into()),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    remap::test_type_def![
        value_noop {
            expr: |_| ParseAwsVpcFlowLogFn::new(Box::new(Noop), None),
            def: TypeDef { fallible: true, kind: Kind::Map, inner_type_def: inner_type_def() },
        }

        value_non_string {
            expr: |_| ParseAwsVpcFlowLogFn::new(Literal::from(1).boxed(), None),
            def: TypeDef { fallible: true, kind: Kind::Map, inner_type_def: inner_type_def() },
        }

        value_string {
            expr: |_| ParseAwsVpcFlowLogFn::new(Literal::from("foo").boxed(), None),
            def: TypeDef { fallible: true, kind: Kind::Map, inner_type_def: inner_type_def() },
        }

        format_non_string {
            expr: |_| ParseAwsVpcFlowLogFn::new(Literal::from("foo").boxed(), Some(Literal::from(1).boxed())),
            def: TypeDef { fallible: true, kind: Kind::Map, inner_type_def: inner_type_def() },
        }
    ];

    #[test]
    fn parse_aws_vpc_flow_log() {
        // Examples from https://docs.aws.amazon.com/vpc/latest/userguide/flow-logs-records-examples.html
        let logs = vec![(
            None,
            vec![
                "2 123456789010 eni-1235b8ca123456789 172.31.16.139 172.31.16.21 20641 22 6 20 4249 1418530010 1418530070 ACCEPT OK",
                "2 123456789010 eni-1235b8ca123456789 172.31.9.69 172.31.9.12 49761 3389 6 20 4249 1418530010 1418530070 REJECT OK",
                "2 123456789010 eni-1235b8ca123456789 - - - - - - - 1431280876 1431280934 - NODATA",
                "2 123456789010 eni-11111111aaaaaaaaa - - - - - - - 1431280876 1431280934 - SKIPDATA",
                "2 123456789010 eni-1235b8ca123456789 203.0.113.12 172.31.16.139 0 0 1 4 336 1432917027 1432917142 ACCEPT OK",
                "2 123456789010 eni-1235b8ca123456789 172.31.16.139 203.0.113.12 0 0 1 4 336 1432917094 1432917142 REJECT OK",
                "2 123456789010 eni-1235b8ca123456789 2001:db8:1234:a100:8d6e:3477:df66:f105 2001:db8:1234:a102:3304:8879:34cf:4071 34892 22 6 54 8855 1477913708 1477913820 ACCEPT OK",
            ]
        ), (
            Some("version vpc_id subnet_id instance_id interface_id account_id type srcaddr dstaddr srcport dstport pkt_srcaddr pkt_dstaddr protocol bytes packets start end action tcp_flags log_status"),
            vec![
                "3 vpc-abcdefab012345678 subnet-aaaaaaaa012345678 i-01234567890123456 eni-1235b8ca123456789 123456789010 IPv4 52.213.180.42 10.0.0.62 43416 5001 52.213.180.42 10.0.0.62 6 568 8 1566848875 1566848933 ACCEPT 2 OK",
                "3 vpc-abcdefab012345678 subnet-aaaaaaaa012345678 i-01234567890123456 eni-1235b8ca123456789 123456789010 IPv4 10.0.0.62 52.213.180.42 5001 43416 10.0.0.62 52.213.180.42 6 376 7 1566848875 1566848933 ACCEPT 18 OK",
                "3 vpc-abcdefab012345678 subnet-aaaaaaaa012345678 i-01234567890123456 eni-1235b8ca123456789 123456789010 IPv4 52.213.180.42 10.0.0.62 43418 5001 52.213.180.42 10.0.0.62 6 100701 70 1566848875 1566848933 ACCEPT 2 OK",
                "3 vpc-abcdefab012345678 subnet-aaaaaaaa012345678 i-01234567890123456 eni-1235b8ca123456789 123456789010 IPv4 10.0.0.62 52.213.180.42 5001 43418 10.0.0.62 52.213.180.42 6 632 12 1566848875 1566848933 ACCEPT 18 OK",
                "3 vpc-abcdefab012345678 subnet-aaaaaaaa012345678 i-01234567890123456 eni-1235b8ca123456789 123456789010 IPv4 10.0.0.62 52.213.180.42 5001 43418 10.0.0.62 52.213.180.42 6 63388 1219 1566848933 1566849113 ACCEPT 1 OK",
                "3 vpc-abcdefab012345678 subnet-aaaaaaaa012345678 i-01234567890123456 eni-1235b8ca123456789 123456789010 IPv4 52.213.180.42 10.0.0.62 43418 5001 52.213.180.42 10.0.0.62 6 23294588 15774 1566848933 1566849113 ACCEPT 1 OK",
                "3 vpc-abcdefab012345678 subnet-aaaaaaaa012345678 i-01234567890123456 eni-1235b8ca123456789 123456789010 IPv4 52.213.180.42 10.0.0.62 43638 5001 52.213.180.42 10.0.0.62 6 1260 17 1566933133 1566933193 ACCEPT 3 OK",
                "3 vpc-abcdefab012345678 subnet-aaaaaaaa012345678 i-01234567890123456 eni-1235b8ca123456789 123456789010 IPv4 10.0.0.62 52.213.180.42 5001 43638 10.0.0.62 52.213.180.42 6 967 14 1566933133 1566933193 ACCEPT 19 OK",
            ]
        ), (
            Some("instance_id interface_id srcaddr dstaddr pkt_srcaddr pkt_dstaddr"),
            vec![
                "- eni-1235b8ca123456789 10.0.1.5 10.0.0.220 10.0.1.5 203.0.113.5",
                "- eni-1235b8ca123456789 10.0.0.220 203.0.113.5 10.0.0.220 203.0.113.5",
                "- eni-1235b8ca123456789 203.0.113.5 10.0.0.220 203.0.113.5 10.0.0.220",
                "- eni-1235b8ca123456789 10.0.0.220 10.0.1.5 203.0.113.5 10.0.1.5",
                "i-01234567890123456 eni-1111aaaa2222bbbb3 10.0.1.5 203.0.113.5 10.0.1.5 203.0.113.5",
                "i-01234567890123456 eni-1111aaaa2222bbbb3 203.0.113.5 10.0.1.5 203.0.113.5 10.0.1.5",
            ]
        ), (
            Some("version interface_id account_id vpc_id subnet_id instance_id srcaddr dstaddr srcport dstport protocol tcp_flags type pkt_srcaddr pkt_dstaddr action log_status"),
            vec![
                "3 eni-33333333333333333 123456789010 vpc-abcdefab012345678 subnet-22222222bbbbbbbbb i-01234567890123456 10.20.33.164 10.40.2.236 39812 80 6 3 IPv4 10.20.33.164 10.40.2.236 ACCEPT OK",
                "3 eni-33333333333333333 123456789010 vpc-abcdefab012345678 subnet-22222222bbbbbbbbb i-01234567890123456 10.40.2.236 10.20.33.164 80 39812 6 19 IPv4 10.40.2.236 10.20.33.164 ACCEPT OK",
                "3 eni-11111111111111111 123456789010 vpc-abcdefab012345678 subnet-11111111aaaaaaaaa - 10.40.1.175 10.40.2.236 39812 80 6 3 IPv4 10.20.33.164 10.40.2.236 ACCEPT OK",
                "3 eni-22222222222222222 123456789010 vpc-abcdefab012345678 subnet-22222222bbbbbbbbb - 10.40.2.236 10.40.2.31 80 39812 6 19 IPv4 10.40.2.236 10.20.33.164 ACCEPT OK",
            ]
        )];

        for (format, logs) in logs {
            for log in logs {
                assert!(parse_log(&log, format).is_ok());
            }
        }
    }
}
