use ::value::Value;
use nom::{
    self,
    branch::alt,
    bytes::complete::{escaped_transform, tag, take_till1, take_until},
    character::complete::{char, one_of, satisfy},
    combinator::{map, opt, value},
    error::{ErrorKind, ParseError, VerboseError},
    multi::{count, many1},
    sequence::{delimited, pair, preceded},
    IResult,
};
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseCef;

impl Function for ParseCef {
    fn identifier(&self) -> &'static str {
        "parse_cef"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "only header",
                source: r#"parse_cef!("CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|")"#,
                result: Ok(
                    r#"{"cefVersion":"1","deviceVendor":"Security","deviceProduct":"threatmanager","deviceVersion":"1.0","deviceEventClassId":"100","name":"worm successfully stopped","severity":"10"}"#,
                ),
            },
            Example {
                title: "header and extension",
                source: r#"parse_cef!("CEF:0|CyberArk|PTA|12.6|1|Suspected credentials theft|8|suser=mike2@prod1.domain.com shost=prod1.domain.com src=1.1.1.1")"#,
                result: Ok(
                    r#"{"cefVersion":"0","deviceVendor":"CyberArk","deviceProduct":"PTA","deviceVersion":"12.6","deviceEventClassId":"1","name":"Suspected credentials theft","severity":"8","suser":"mike2@prod1.domain.com","shost":"prod1.domain.com","src":"1.1.1.1"}"#,
                ),
            },
            Example {
                title: "with syslog prefix",
                source: r#"parse_cef!("Sep 29 08:26:10 host CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|")"#,
                result: Ok(
                    r#"{"cefVersion":"1","deviceVendor":"Security","deviceProduct":"threatmanager","deviceVersion":"1.0","deviceEventClassId":"100","name":"worm successfully stopped","severity":"10"}"#,
                ),
            },
            Example {
                title: "escapes",
                source: r#"parse_cef!(s'CEF:0|security|threatmanager|1.0|100|Detected a \| in message. No action needed.|10|src=10.0.0.1 msg=Detected a threat.\n No action needed act=blocked a \= dst=1.1.1.1')"#,
                result: Ok(
                    r#"{"cefVersion":"0","deviceVendor":"security","deviceProduct":"threatmanager","deviceVersion":"1.0","deviceEventClassId":"100","name":"Detected a | in message. No action needed.","severity":"10","src":"10.0.0.1","msg":"Detected a threat.\n No action needed","act":"blocked a =", "dst":"1.1.1.1"}"#,
                ),
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

        Ok(ParseCefFn { value }.as_expr())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParseCefFn {
    pub(crate) value: Box<dyn Expression>,
}

impl FunctionExpression for ParseCefFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        let bytes = bytes.try_bytes_utf8_lossy()?;

        parse(&bytes).map(Iterator::collect)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        type_def()
    }
}

fn parse(input: &str) -> Result<impl Iterator<Item = (String, Value)> + '_> {
    let (rest, (header, mut extension)) =
        pair(parse_header, parse_extension)(input).map_err(|e| match e {
            nom::Err::Error(e) | nom::Err::Failure(e) => {
                // Create a descriptive error message if possible.
                nom::error::convert_error(input, e)
            }
            nom::Err::Incomplete(_) => e.to_string(),
        })?;

    // Trim trailing whitespace on last extension value
    if let Some((_, value)) = extension.last_mut() {
        let suffix = value.trim_end_matches(' ');
        value.truncate(suffix.len());
    }

    if rest.trim().is_empty() {
        let headers = [
            "cefVersion",
            "deviceVendor",
            "deviceProduct",
            "deviceVersion",
            "deviceEventClassId",
            "name",
            "severity",
        ]
        .into_iter()
        .zip(header);
        let result = extension
            .into_iter()
            .chain(headers)
            .map(|(key, value)| (key.to_string(), value.into()));

        Ok(result)
    } else {
        Err("Could not parse whole line successfully".into())
    }
}

fn parse_header(input: &str) -> IResult<&str, Vec<String>, VerboseError<&str>> {
    preceded(
        pair(take_until("CEF:"), tag("CEF:")),
        count(parse_header_value, 7),
    )(input)
}

fn parse_header_value(input: &str) -> IResult<&str, String, VerboseError<&str>> {
    preceded(
        opt(char('|')),
        escaped_transform(
            take_till1(|c: char| c == '\\' || c == '|'),
            '\\',
            satisfy(|c| c == '\\' || c == '|'),
        ),
    )(input)
}

fn parse_extension(input: &str) -> IResult<&str, Vec<(&str, String)>, VerboseError<&str>> {
    alt((many1(parse_key_value), map(tag("|"), |_| vec![])))(input)
}

fn parse_key_value(input: &str) -> IResult<&str, (&str, String), VerboseError<&str>> {
    pair(parse_key, parse_value)(input)
}

fn parse_value(input: &str) -> IResult<&str, String, VerboseError<&str>> {
    escaped_transform(
        take_till1_input(|input| alt((tag("\\"), tag("="), parse_key))(input).is_ok()),
        '\\',
        alt((
            value('=', char('=')),
            value('\\', char('\\')),
            value('\n', one_of("nr")),
        )),
    )(input)
}

/// As take take_till1 but can have condition on input instead of Input::Item.
fn take_till1_input<'a, F: Fn(&'a str) -> bool, Error: ParseError<&'a str>>(
    cond: F,
) -> impl Fn(&'a str) -> IResult<&'a str, &'a str, Error> {
    move |input: &'a str| {
        for (i, _) in input.char_indices() {
            if cond(&input[i..]) {
                return if i == 0 {
                    Err(nom::Err::Error(Error::from_error_kind(
                        input,
                        ErrorKind::TakeTill1,
                    )))
                } else {
                    Ok((&input[i..], &input[..i]))
                };
            }
        }
        Ok(("", input))
    }
}

fn parse_key(input: &str) -> IResult<&str, &str, VerboseError<&str>> {
    delimited(
        alt((char(' '), char('|'))),
        take_till1(|c| c == ' ' || c == '=' || c == '\\'),
        char('='),
    )(input)
}

fn type_def() -> TypeDef {
    TypeDef::object(Collection::from_parts(
        BTreeMap::from([
            (Field::from("cefVersion"), Kind::bytes()),
            (Field::from("deviceVendor"), Kind::bytes()),
            (Field::from("deviceProduct"), Kind::bytes()),
            (Field::from("deviceVersion"), Kind::bytes()),
            (Field::from("deviceEventClassId"), Kind::bytes()),
            (Field::from("name"), Kind::bytes()),
            (Field::from("severity"), Kind::bytes()),
        ]),
        Kind::bytes(),
    ))
    .fallible()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_header() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
            ]),
            parse("CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|")
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_parse_extension() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
                ("src".to_string(), "10.0.0.1".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("spt".to_string(),"1232".into())
            ]),
            parse("CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|src=10.0.0.1 dst=2.1.2.2 spt=1232")
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_ignore_syslog_prefix() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
                ("src".to_string(), "10.0.0.1".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("spt".to_string(),"1232".into())
            ]),
            parse("Sep 29 08:26:10 host CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|src=10.0.0.1 dst=2.1.2.2 spt=1232")
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_escape_header_1() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm | successfully | stopped".into()),
                ("severity".to_string(), "10".into()),
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm \| successfully \| stopped|10|"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_escape_header_2() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm \\ successfully \\ stopped".into()),
                ("severity".to_string(), "10".into()),
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm \\ successfully \\ stopped|10|"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_escape_extension_1() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
                ("src".to_string(), "ip=10.0.0.1".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("spt".to_string(),"1232".into())
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|src=ip\=10.0.0.1 dst=2.1.2.2 spt=1232"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_escape_extension_2() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("path".to_string(), "\\home\\".into()),
                ("spt".to_string(),"1232".into())
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|dst=2.1.2.2 path=\\home\\ spt=1232"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_extension_newline() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("msg".to_string(), "Detected a threat.\n No action needed".into()),
                ("spt".to_string(),"1232".into())
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|dst=2.1.2.2 msg=Detected a threat.\r No action needed spt=1232"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_extension_trailing_whitespace() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("msg".to_string(), "Detected a threat. No action needed  ".into()),
                ("spt".to_string(),"1232".into())
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|dst=2.1.2.2 msg=Detected a threat. No action needed   spt=1232"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_extension_end_whitespace() {
        assert_eq!(
            Ok(vec![
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("msg".to_string(), "Detected a threat. No action needed".into()),
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|dst=2.1.2.2 msg=Detected a threat. No action needed   "#)
                .map(Iterator::collect)
        );
    }

    test_function![
        parse_cef => ParseCef;

        default {
            args: func_args! [
                value: r#"CEF:0|CyberArk|PTA|12.6|1|Suspected credentials theft|8|suser=mike2@prod1.domain.com shost=prod1.domain.com src=1.1.1.1"#,
            ],
            want: Ok(value!({
                "cefVersion":"0",
                "deviceVendor":"CyberArk",
                "deviceProduct":"PTA",
                "deviceVersion":"12.6",
                "deviceEventClassId":"1",
                "name":"Suspected credentials theft",
                "severity":"8",
                "suser":"mike2@prod1.domain.com",
                "shost":"prod1.domain.com","src":"1.1.1.1"
            })),
            tdef: type_def(),
        }

        real_case {
            args: func_args! [
                value: r#"CEF:0|Check Point|VPN-1 & FireWall-1|Check Point|Log|https|Unknown|act=Accept destinationTranslatedAddress=0.0.0.0 destinationTranslatedPort=0 deviceDirection=0 rt=1543270652000 sourceTranslatedAddress=192.168.103.254 sourceTranslatedPort=35398 spt=49363 dpt=443 cs2Label=Rule Name layer_name=Network layer_uuid=b406b732-2437-4848-9741-6eae1f5bf112 match_id=4 parent_rule=0 rule_action=Accept rule_uid=9e5e6e74-aa9a-4693-b9fe-53712dd27bea ifname=eth0 logid=0 loguid={0x5bfc70fc,0x1,0xfe65a8c0,0xc0000001} origin=192.168.101.254 originsicname=CN\=R80,O\=R80_M..6u6bdo sequencenum=1 version=5 dst=52.173.84.157 inzone=Internal nat_addtnl_rulenum=1 nat_rulenum=4 outzone=External product=VPN-1 & FireWall-1 proto=6 service_id=https src=192.168.101.100"#,
            ],
            want: Ok(value!({
                "cefVersion":"0",
                "deviceVendor":"Check Point",
                "deviceProduct":"VPN-1 & FireWall-1",
                "deviceVersion":"Check Point",
                "deviceEventClassId":"Log",
                "name":"https",
                "severity":"Unknown",
                "act": "Accept",
                "destinationTranslatedAddress": "0.0.0.0",
                "destinationTranslatedPort": "0",
                "deviceDirection": "0",
                "rt": "1543270652000",
                "sourceTranslatedAddress": "192.168.103.254",
                "sourceTranslatedPort": "35398",
                "spt": "49363",
                "dpt": "443",
                "cs2Label": "Rule Name",
                "layer_name": "Network",
                "layer_uuid": "b406b732-2437-4848-9741-6eae1f5bf112",
                "match_id": "4",
                "parent_rule": "0",
                "rule_action": "Accept",
                "rule_uid": "9e5e6e74-aa9a-4693-b9fe-53712dd27bea",
                "ifname": "eth0",
                "logid": "0",
                "loguid": "{0x5bfc70fc,0x1,0xfe65a8c0,0xc0000001}",
                "origin": "192.168.101.254",
                "originsicname": "CN=R80,O=R80_M..6u6bdo",
                "sequencenum": "1",
                "version": "5",
                "dst": "52.173.84.157",
                "inzone": "Internal",
                "nat_addtnl_rulenum": "1",
                "nat_rulenum": "4",
                "outzone": "External",
                "product": "VPN-1 & FireWall-1",
                "proto": "6",
                "service_id": "https",
                "src": "192.168.101.100",
            })),
            tdef: type_def(),
        }

        missing_key {
            args: func_args! [
                value: r#"CEF:0|Check Point|VPN-1 & FireWall-1|Check Point|Log|https|Unknown|act=Accept =0.0.0.0"#,
            ],
            want: Err("Could not parse whole line successfully"),
            tdef: type_def(),
        }

        incomplete_header {
            args: func_args! [
                value: r#"CEF:0|Check Point|VPN-1 & FireWall-1|Check Point|Log|https|"#,
            ],
            want: Err("0: at line 1, in Tag:\nCEF:0|Check Point|VPN-1 & FireWall-1|Check Point|Log|https|\n                                                           ^\n\n1: at line 1, in Alt:\nCEF:0|Check Point|VPN-1 & FireWall-1|Check Point|Log|https|\n                                                           ^\n\n"),
            tdef: type_def(),
        }

    ];
}
