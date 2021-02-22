use nom::{
    self,
    branch::alt,
    bytes::complete::{escaped, tag, take_until, take_while1},
    character::complete::{char, satisfy, space0},
    combinator::{map, rest},
    error::{ContextError, ParseError, VerboseError},
    multi::{many1, separated_list1},
    sequence::{delimited, preceded, tuple},
    IResult,
};
use std::iter::FromIterator;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseKeyValue;

impl Function for ParseKeyValue {
    fn identifier(&self) -> &'static str {
        "parse_key_value"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "key_value_delimiter",
                kind: kind::ANY,
                required: false,
            },
            Parameter {
                keyword: "field_delimiter",
                kind: kind::ANY,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "simple key value",
                source: r#"parse_key_value!("zork=zook zonk=nork")"#,
                result: Ok(r#"{"zork": "zook", "zonk": "nork"}"#),
            },
            Example {
                title: "custom delimiters",
                source: r#"parse_key_value!(s'zork: zoog, nonk: "nink nork"', key_value_delimiter: ":", field_delimiter: ",")"#,
                result: Ok(r#"{"zork": "zoog", "nonk": "nink nork"}"#),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        let key_value_delimiter = arguments
            .optional("key_value_delimiter")
            .unwrap_or_else(|| expr!("="));

        let field_delimiter = arguments
            .optional("field_delimiter")
            .unwrap_or_else(|| expr!(" "));

        Ok(Box::new(ParseKeyValueFn {
            value,
            key_value_delimiter,
            field_delimiter,
        }))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParseKeyValueFn {
    value: Box<dyn Expression>,
    key_value_delimiter: Box<dyn Expression>,
    field_delimiter: Box<dyn Expression>,
}

impl Expression for ParseKeyValueFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let bytes = value.unwrap_bytes_utf8_lossy();

        let value = self.key_value_delimiter.resolve(ctx)?;
        let key_value_delimiter = value.unwrap_bytes_utf8_lossy();

        let value = self.field_delimiter.resolve(ctx)?;
        let field_delimiter = value.unwrap_bytes_utf8_lossy();

        let values = parse(&bytes, &key_value_delimiter, &field_delimiter)?;

        Ok(Value::from_iter(values))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().object::<(), Kind>(map! {
            (): Kind::all()
        })
    }
}

fn parse<'a>(
    input: &'a str,
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
) -> Result<Vec<(String, Value)>> {
    let (rest, result) =
        parse_line(input, key_value_delimiter, field_delimiter).map_err(|e| match e {
            nom::Err::Error(e) | nom::Err::Failure(e) => {
                // Create a descriptive error message if possible.
                nom::error::convert_error(input, e)
            }
            _ => format!("{}", e),
        })?;

    if rest.trim().is_empty() {
        Ok(result)
    } else {
        Err("could not parse whole line successfully".into())
    }
}

/// Parse the line as a separated list of key value pairs.
fn parse_line<'a>(
    input: &'a str,
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
) -> IResult<&'a str, Vec<(String, Value)>, VerboseError<&'a str>> {
    separated_list1(
        parse_field_delimiter(field_delimiter),
        parse_key_value(key_value_delimiter, field_delimiter),
    )(input)
}

/// Parses the field_delimiter between the key/value pairs.
/// If the field_delimiter is a space, we parse as many as we can,
/// If it is not a space eat any whitespace before our field_delimiter as well as the field_delimiter.
fn parse_field_delimiter<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    field_delimiter: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, &'a str, E> {
    move |input| {
        if field_delimiter == " " {
            map(many1(tag(field_delimiter)), |_| " ")(input)
        } else {
            preceded(space0, tag(field_delimiter))(input)
        }
    }
}

/// Parse a single `key=value` tuple.
fn parse_key_value<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, (String, Value), E> {
    move |input| {
        map(
            tuple((
                preceded(space0, parse_key(key_value_delimiter)),
                preceded(space0, tag(key_value_delimiter)),
                preceded(space0, parse_value(field_delimiter)),
            )),
            |(field, _, value): (&str, &str, Value)| (field.to_string(), value),
        )(input)
    }
}

/// Parses a string delimited by the given character.
/// Can be escaped using `\`.
fn parse_delimited<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    delimiter: char,
) -> impl Fn(&'a str) -> IResult<&'a str, &'a str, E> {
    move |input| {
        delimited(
            char(delimiter),
            escaped(
                take_while1(|c: char| c != '\\' && c != delimiter),
                '\\',
                satisfy(|c| c == '\\' || c == delimiter),
            ),
            char(delimiter),
        )(input)
    }
}

/// An undelimited value is all the text until our field_delimiter, or if it is the last value in the line,
/// just take the rest of the string.
fn parse_undelimited<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    field_delimiter: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, &'a str, E> {
    move |input| map(alt((take_until(field_delimiter), rest)), |s: &str| s.trim())(input)
}

/// Parses the value.
/// The value has two parsing strategies.
///
/// 1. Parse as a delimited field - currently the delimiter is hardcoded to a `"`.
/// 2. If it does not start with one of the trim values, it is not a delimited field and we parse up to
///    the next field_delimiter or the eof.
///
fn parse_value<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    field_delimiter: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, Value, E> {
    move |input| {
        map(
            alt((parse_delimited('"'), parse_undelimited(field_delimiter))),
            Into::into,
        )(input)
    }
}

/// Parses the key.
/// Parsing strategies are the same as parse_value, but we don't need to convert the result to a `Value`.
fn parse_key<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    field_delimiter: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, &'a str, E> {
    move |input| alt((parse_delimited('"'), parse_undelimited(field_delimiter)))(input)
}

/*
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse() {
        assert_eq!(
            Ok(vec![
                ("ook".to_string(), "pook".into()),
                (
                    "@timestamp".to_string(),
                    "2020-12-31T12:43:22.2322232Z".into()
                ),
                ("key#hash".to_string(), "value".into()),
                ("key=with=special=characters".to_string(), "value".into()),
                ("key".to_string(), "with special=characters".into()),
            ]),
            parse(
                r#"ook=pook @timestamp=2020-12-31T12:43:22.2322232Z key#hash=value "key=with=special=characters"=value key="with special=characters""#,
                "=",
                " "
            )
        );
    }

    #[test]
    fn test_parse_key_value() {
        assert_eq!(
            Ok(("", ("ook".to_string(), "pook".into()))),
            parse_key_value::<VerboseError<&str>>("=", " ")("ook=pook")
        );
    }

    #[test]
    fn test_parse_value() {
        // delimited
        assert_eq!(
            Ok(("", "noog".into())),
            parse_value::<VerboseError<&str>>(" ")(r#""noog""#)
        );

        // undelimited
        assert_eq!(
            Ok(("", "noog".into())),
            parse_value::<VerboseError<&str>>(" ")("noog")
        );
    }

    test_function![
        parse_key_value => ParseKeyValue;

        default {
            args: func_args! [
                value: r#"at=info method=GET path=/ host=myapp.herokuapp.com request_id=8601b555-6a83-4c12-8269-97c8e32cdb22 fwd="204.204.204.204" dyno=web.1 connect=1ms service=18ms status=200 bytes=13 tls_version=tls1.1 protocol=http"#,
            ],
            want: Ok(value!({at: "info",
                             method: "GET",
                             path: "/",
                             host: "myapp.herokuapp.com",
                             request_id: "8601b555-6a83-4c12-8269-97c8e32cdb22",
                             fwd: "204.204.204.204",
                             dyno: "web.1",
                             connect: "1ms",
                             service: "18ms",
                             status: "200",
                             bytes: "13",
                             tls_version: "tls1.1",
                             protocol: "http"})),
            tdef: TypeDef::new().fallible().object(map!{}),
        }

        logfmt {
            args: func_args! [
                value: r#"level=info msg="Stopping all fetchers" tag=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager"#
            ],
            want: Ok(value!({level: "info",
                             msg: "Stopping all fetchers",
                             tag: "stopping_fetchers",
                             id: "ConsumerFetcherManager-1382721708341",
                             module: "kafka.consumer.ConsumerFetcherManager"})),
            tdef: TypeDef::new().fallible().object(map!{}),
        }

        // From https://github.com/timberio/vector/issues/5347
        real_case {
            args: func_args! [
                value: r#"SerialNum=100018002000001906146520 GenTime="2019-10-24 14:25:03" SrcIP=10.10.254.2 DstIP=10.10.254.7 Protocol=UDP SrcPort=137 DstPort=137 PolicyID=3 Action=PERMIT Content="Session Backout""#
            ],
            want: Ok(value!({SerialNum: "100018002000001906146520",
                             GenTime: "2019-10-24 14:25:03",
                             SrcIP: "10.10.254.2",
                             DstIP: "10.10.254.7",
                             Protocol: "UDP",
                             SrcPort: "137",
                             DstPort: "137",
                             PolicyID: "3",
                             Action: "PERMIT",
                             Content: "Session Backout"})),
            tdef: TypeDef::new().fallible().object(map!{}),
        }

        spaces {
            args: func_args! [
                value: r#""zork one" : "zoog\"zink\"zork"        nonk          : nink"#,
                key_value_delimiter: ":",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             nonk: "nink"})),
            tdef: TypeDef::new().fallible().object(map!{}),
        }

        delimited {
            args: func_args! [
                value: r#""zork one":"zoog\"zink\"zork", nonk:nink"#,
                key_value_delimiter: ":",
                field_delimiter: ",",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             nonk: "nink"})),
            tdef: TypeDef::new().fallible().object(map!{}),
        }

        delimited_with_spaces {
            args: func_args! [
                value: r#""zork one" : "zoog\"zink\"zork"  ,      nonk          : nink"#,
                key_value_delimiter: ":",
                field_delimiter: ",",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             nonk: "nink"})),
            tdef: TypeDef::new().fallible().object(map!{}),
        }

        multiple_chars {
            args: func_args! [
                value: r#""zork one" -- "zoog\"zink\"zork"  ||    nonk          -- nink"#,
                key_value_delimiter: "--",
                field_delimiter: "||",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             nonk: "nink"})),
            tdef: TypeDef::new().fallible().object(map!{}),
        }

        error {
            args: func_args! [
                value: r#"I am not a valid line."#,
                key_value_delimiter: "--",
                field_delimiter: "||",
            ],
            want: Err("function call error: 0: at line 1, in Tag:\nI am not a valid line.\n                      ^\n\n"),
            tdef: TypeDef::new().fallible().object(map!{}),
        }

        // The following case demonstrates a scenario that could potentially be considered an
        // error, but isn't. It is possible that we are missing a separator here (between nink and
        // norgle), but it parses it successfully and just assumes all the text after the
        // key_value_delimiter is the value since there is no terminator to stop the parsing.
        missing_separator {
            args: func_args! [
                value: r#"zork: zoog, nonk: nink norgle: noog"#,
                key_value_delimiter: ":",
                field_delimiter: ",",
            ],
            want: Ok(value!({zork: r#"zoog"#,
                             nonk: "nink norgle: noog"})),
            tdef: TypeDef::new().fallible().object(map!{}),
        }

        // If the value field is delimited, then it can't parse the rest of the
        // line, so it raises an error.
        missing_separator_delimited {
            args: func_args! [
                value: r#"zork: zoog, nonk: "nink" norgle: noog"#,
                key_value_delimiter: ":",
                field_delimiter: ",",
            ],
            want: Err("function call error: could not parse whole line successfully"),
            tdef: TypeDef::new().fallible().object(map!{}),
        }
    ];
}
*/
