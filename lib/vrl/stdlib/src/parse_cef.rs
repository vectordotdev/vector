use std::collections::HashMap;

use ::value::Value;
use nom::{
    self,
    branch::alt,
    bytes::complete::{escaped_transform, tag, take_till1, take_until},
    character::complete::{char, one_of, satisfy},
    combinator::{map, opt, peek, success, value},
    error::{ErrorKind, ParseError, VerboseError},
    multi::{count, many1},
    sequence::{delimited, pair, preceded},
    IResult,
};
use vrl::prelude::*;

fn build_map() -> HashMap<&'static str, (usize, CustomField)> {
    [
        ("c6a1Label", "c6a1"),
        ("c6a2Label", "c6a2"),
        ("c6a3Label", "c6a3"),
        ("c6a4Label", "c6a4"),
        ("cfp1Label", "cfp1"),
        ("cfp2Label", "cfp2"),
        ("cfp3Label", "cfp3"),
        ("cfp4Label", "cfp4"),
        ("cn1Label", "cn1"),
        ("cn2Label", "cn2"),
        ("cn3Label", "cn3"),
        ("cs1Label", "cs1"),
        ("cs2Label", "cs2"),
        ("cs3Label", "cs3"),
        ("cs4Label", "cs4"),
        ("cs5Label", "cs5"),
        ("cs6Label", "cs6"),
        ("deviceCustomDate1Label", "deviceCustomDate1"),
        ("deviceCustomDate2Label", "deviceCustomDate2"),
        ("flexDate1Label", "flexDate1"),
        ("flexString1Label", "flexString1"),
        ("flexString2Label", "flexString2"),
    ]
    .iter()
    .enumerate()
    .flat_map(|(i, (k, v))| [(*k, (i, CustomField::Label)), (*v, (i, CustomField::Value))])
    .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct ParseCef;

impl Function for ParseCef {
    fn identifier(&self) -> &'static str {
        "parse_cef"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "translate_custom_fields",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
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
                source: r#"parse_cef!("CEF:0|CyberArk|PTA|12.6|1|\"Suspected credentials theft\"|8|suser=mike2@prod1.domain.com shost=prod1.domain.com src=1.1.1.1")"#,
                result: Ok(
                    r#"{"cefVersion":"0","deviceVendor":"CyberArk","deviceProduct":"PTA","deviceVersion":"12.6","deviceEventClassId":"1","name":"Suspected credentials theft","severity":"8","suser":"mike2@prod1.domain.com","shost":"prod1.domain.com","src":"1.1.1.1"}"#,
                ),
            },
            Example {
                title: "empty value",
                source: r#"parse_cef!("CEF:0|CyberArk|PTA|12.6|1|Suspected credentials theft||suser=mike2@prod1.domain.com shost= src=1.1.1.1")"#,
                result: Ok(
                    r#"{"cefVersion":"0","deviceVendor":"CyberArk","deviceProduct":"PTA","deviceVersion":"12.6","deviceEventClassId":"1","name":"Suspected credentials theft","severity":"","suser":"mike2@prod1.domain.com","shost":"","src":"1.1.1.1"}"#,
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
            Example {
                title: "translate custom fields",
                source: r#"parse_cef!("CEF:0|CyberArk|PTA|12.6|1|\"Suspected credentials theft\"|8|suser=mike2@prod1.domain.com shost=prod1.domain.com c6a1=2345:0425:2CA1:0000:0000:0567:5673:23b5 c6a1Label=Device IPv6 Address", translate_custom_fields: true)"#,
                result: Ok(
                    r#"{"cefVersion":"0","deviceVendor":"CyberArk","deviceProduct":"PTA","deviceVersion":"12.6","deviceEventClassId":"1","name":"Suspected credentials theft","severity":"8","suser":"mike2@prod1.domain.com","shost":"prod1.domain.com","Device IPv6 Address":"2345:0425:2CA1:0000:0000:0567:5673:23b5"}"#,
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
        let translate_custom_fields = arguments.optional("translate_custom_fields");
        let custom_field_map = build_map();

        Ok(ParseCefFn {
            value,
            translate_custom_fields,
            custom_field_map,
        }
        .as_expr())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParseCefFn {
    pub(crate) value: Box<dyn Expression>,
    pub(crate) translate_custom_fields: Option<Box<dyn Expression>>,
    custom_field_map: HashMap<&'static str, (usize, CustomField)>,
}

impl FunctionExpression for ParseCefFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?;
        let bytes = bytes.try_bytes_utf8_lossy()?;
        let translate_custom_fields = if let Some(field) = self.translate_custom_fields.as_ref() {
            field.resolve(ctx)?.try_boolean()?
        } else {
            false
        };

        let result = parse(&bytes)?;

        if translate_custom_fields {
            let mut custom_fields = HashMap::<_, [Option<String>; 2]>::new();

            let mut result = result
                .filter_map(|(k, v)| {
                    if let Some(&(i, custom_field)) = self.custom_field_map.get(k.as_str()) {
                        let previous =
                            custom_fields.entry(i).or_default()[custom_field as usize].replace(v);
                        if previous.is_some() {
                            return Some(Err(format!(
                                "Custom field with duplicate {}",
                                match custom_field {
                                    CustomField::Label => "label",
                                    CustomField::Value => "value",
                                }
                            )
                            .into()));
                        }
                        None
                    } else {
                        Some(Ok((k, v.into())))
                    }
                })
                .collect::<Result<BTreeMap<String, Value>>>()?;

            for (_, fields) in custom_fields {
                match fields {
                    [Some(label), value] => {
                        result.insert(label, value.into());
                    }
                    _ => return Err("Custom field with missing label or value".into()),
                }
            }

            Ok(Value::Object(result))
        } else {
            Ok(result.map(|(k, v)| (k, v.into())).collect())
        }
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        type_def()
    }
}

#[derive(Debug, Clone, Copy)]
enum CustomField {
    Label = 0,
    Value = 1,
}

fn parse(input: &str) -> Result<impl Iterator<Item = (String, String)> + '_> {
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
            .map(|(key, mut value)| {
                // Strip quotes from value
                if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
                    value = value[1..value.len() - 1].to_string();
                }
                (key, value)
            })
            .map(|(key, value)| (key.to_string(), value));

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
        alt((
            map(peek(char('|')), |_| String::new()),
            escaped_transform(
                take_till1(|c: char| c == '\\' || c == '|'),
                '\\',
                satisfy(|c| c == '\\' || c == '|'),
            ),
        )),
    )(input)
}

fn parse_extension(input: &str) -> IResult<&str, Vec<(&str, String)>, VerboseError<&str>> {
    alt((many1(parse_key_value), map(tag("|"), |_| vec![])))(input)
}

fn parse_key_value(input: &str) -> IResult<&str, (&str, String), VerboseError<&str>> {
    pair(parse_key, parse_value)(input)
}

fn parse_value(input: &str) -> IResult<&str, String, VerboseError<&str>> {
    alt((
        map(peek(parse_key), |_| String::new()),
        escaped_transform(
            take_till1_input(|input| alt((tag("\\"), tag("="), parse_key))(input).is_ok()),
            '\\',
            alt((
                value('=', char('=')),
                value('\\', char('\\')),
                value('\n', one_of("nr")),
                success('\\'),
            )),
        ),
    ))(input)
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
                ("src".to_string(), "10.0.0.1".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("spt".to_string(),"1232".into()),
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
            ]),
            parse("CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|src=10.0.0.1 dst=2.1.2.2 spt=1232")
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_parse_empty_value() {
        assert_eq!(
            Ok(vec![
                ("src".to_string(), String::new()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), String::new()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), String::new()),
            ]),
            parse("CEF:1|Security|threatmanager||100|worm successfully stopped||src= dst=2.1.2.2")
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(
            Ok(vec![
                ("src".to_string(), "10.0.0.1".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("spt".to_string(),"1232".into()),
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
            ]),
            parse(r#"CEF:1|"Security"|threatmanager|1.0|100|"worm successfully stopped"|10|src="10.0.0.1" dst=2.1.2.2 spt="1232""#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_ignore_syslog_prefix() {
        assert_eq!(
            Ok(vec![
                ("src".to_string(), "10.0.0.1".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("spt".to_string(),"1232".into()),
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
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
                ("src".to_string(), "ip=10.0.0.1".into()),
                ("dst".to_string(), "2.1.2.2".into()),
                ("spt".to_string(),"1232".into()),
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|src=ip\=10.0.0.1 dst=2.1.2.2 spt=1232"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_escape_extension_2() {
        assert_eq!(
            Ok(vec![
                ("dst".to_string(), "2.1.2.2".into()),
                ("path".to_string(), "\\home\\".into()),
                ("spt".to_string(),"1232".into()),
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|dst=2.1.2.2 path=\\home\\ spt=1232"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_extension_newline() {
        assert_eq!(
            Ok(vec![
                ("dst".to_string(), "2.1.2.2".into()),
                ("msg".to_string(), "Detected a threat.\n No action needed".into()),
                ("spt".to_string(),"1232".into()),
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|dst=2.1.2.2 msg=Detected a threat.\r No action needed spt=1232"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_extension_trailing_whitespace() {
        assert_eq!(
            Ok(vec![
                ("dst".to_string(), "2.1.2.2".into()),
                ("msg".to_string(), "Detected a threat. No action needed  ".into()),
                ("spt".to_string(),"1232".into()),
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
            ]),
            parse(r#"CEF:1|Security|threatmanager|1.0|100|worm successfully stopped|10|dst=2.1.2.2 msg=Detected a threat. No action needed   spt=1232"#)
                .map(Iterator::collect)
        );
    }

    #[test]
    fn test_extension_end_whitespace() {
        assert_eq!(
            Ok(vec![
                ("dst".to_string(), "2.1.2.2".into()),
                ("msg".to_string(), "Detected a threat. No action needed".into()),
                ("cefVersion".to_string(), "1".into()),
                ("deviceVendor".to_string(), "Security".into()),
                ("deviceProduct".to_string(), "threatmanager".into()),
                ("deviceVersion".to_string(), "1.0".into()),
                ("deviceEventClassId".to_string(), "100".into()),
                ("name".to_string(), "worm successfully stopped".into()),
                ("severity".to_string(), "10".into()),
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
                "shost":"prod1.domain.com",
                "src":"1.1.1.1"
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


        translate_custom_fields {
            args: func_args! [
                value: r#"CEF:0|CyberArk|PTA|12.6|1|Suspected credentials theft|8|suser=mike2@prod1.domain.com cn1=1254323565 shost=prod1.domain.com src=1.1.1.1 cfp1Label=Uptime hours cfp1=35.46 cn1Label=Internal ID"#,
                translate_custom_fields: true
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
                "shost":"prod1.domain.com",
                "src":"1.1.1.1",
                "Uptime hours":"35.46",
                "Internal ID":"1254323565",
            })),
            tdef: type_def(),
        }

        missing_value {
            args: func_args! [
                value: r#"CEF:0|CyberArk|PTA|12.6||Suspected credentials theft||suser=mike2@prod1.domain.com shost= src=1.1.1.1"#,
            ],
            want: Ok(value!({
                "cefVersion":"0",
                "deviceVendor":"CyberArk",
                "deviceProduct":"PTA",
                "deviceVersion":"12.6",
                "deviceEventClassId":"",
                "name":"Suspected credentials theft",
                "severity":"",
                "suser":"mike2@prod1.domain.com",
                "shost":"",
                "src":"1.1.1.1"
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

        utf8_escape {
            args: func_args! [
                value: r#"CEF:0|xxx|xxx|123456|xxx|xxx|5|TestField={'blabla': 'blabla\xc3\xaablabla'}"#,
            ],
            want: Ok(value!({
                "cefVersion":"0",
                "deviceVendor":"xxx",
                "deviceProduct":"xxx",
                "deviceVersion":"123456",
                "deviceEventClassId":"xxx",
                "name":"xxx",
                "severity":"5",
                "TestField": r#"{'blabla': 'blabla\xc3\xaablabla'}"#,
            })),
            tdef: type_def(),
        }

        missing_custom_label {
            args: func_args! [
                value: r#"CEF:0|CyberArk|PTA|12.6|1|Suspected credentials theft|8|cfp1=1.23"#,
                translate_custom_fields: true
            ],
            want: Err("Custom field with missing label or value"),
            tdef: type_def(),
        }

        duplicate_value {
            args: func_args! [
                value: r#"CEF:0|CyberArk|PTA|12.6|1|Suspected credentials theft|8|flexString1=1.23 flexString1=1.24 flexString1Label=Version"#,
                translate_custom_fields: true
            ],
            want: Err("Custom field with duplicate value"),
            tdef: type_def(),
        }

    ];
}
