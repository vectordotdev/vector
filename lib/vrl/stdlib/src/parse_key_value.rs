use std::{iter::FromIterator, str::FromStr};

use nom::{
    self,
    branch::alt,
    bytes::complete::{escaped, tag, take_until, take_while1},
    character::complete::{char, satisfy, space0},
    combinator::{eof, map, opt, peek, recognize, rest, verify},
    error::{ContextError, ParseError, VerboseError},
    multi::{many1, many_m_n, separated_list1},
    sequence::{delimited, preceded, terminated, tuple},
    IResult,
};
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
            Parameter {
                keyword: "whitespace",
                kind: kind::BYTES,
                required: false,
            },
            Parameter {
                keyword: "accept_standalone_key",
                kind: kind::BOOLEAN,
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
            Example {
                title: "strict whitespace",
                source: r#"parse_key_value!(s'app=my-app ip=1.2.3.4 user= msg=hello-world', whitespace: "strict")"#,
                result: Ok(
                    r#"{"app": "my-app", "ip": "1.2.3.4", "user": "", "msg": "hello-world"}"#,
                ),
            },
            Example {
                title: "standalone key",
                source: r#"parse_key_value!(s'foo=bar foobar', whitespace: "strict")"#,
                result: Ok(r#"{"foo": "bar", "foobar": true}"#),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        let key_value_delimiter = arguments
            .optional("key_value_delimiter")
            .unwrap_or_else(|| expr!("="));

        let field_delimiter = arguments
            .optional("field_delimiter")
            .unwrap_or_else(|| expr!(" "));

        let whitespace = arguments
            .optional_enum("whitespace", &Whitespace::all_value())?
            .map(|s| {
                Whitespace::from_str(&s.try_bytes_utf8_lossy().expect("whitespace not bytes"))
                    .expect("validated enum")
            })
            .unwrap_or_default();

        let standalone_key = arguments
            .optional("accept_standalone_key")
            .unwrap_or_else(|| expr!(true));

        Ok(Box::new(ParseKeyValueFn {
            value,
            key_value_delimiter,
            field_delimiter,
            whitespace,
            standalone_key,
        }))
    }

    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<FunctionArgument>)],
        _info: &FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        match (name, expr) {
            ("whitespace", Some(expr)) => match expr.as_value() {
                None => Ok(None),
                Some(value) => Ok(Some(
                    Whitespace::from_str(
                        &value.try_bytes_utf8_lossy().expect("whitespace not bytes"),
                    )
                    .map(|whitespace| Box::new(whitespace) as Box<dyn std::any::Any + Send + Sync>)
                    .map_err(|_| vrl::function::Error::InvalidEnumVariant {
                        keyword: "whitespace",
                        value,
                        variants: Whitespace::all_value().to_vec(),
                    })?,
                )),
            },
            _ => Ok(None),
        }
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        let bytes = value.try_bytes_utf8_lossy()?;

        let key_value_delimiter = args
            .optional("key_value_delimiter")
            .unwrap_or_else(|| value!("="));
        let key_value_delimiter = key_value_delimiter.try_bytes_utf8_lossy()?;

        let field_delimiter = args
            .optional("field_delimiter")
            .unwrap_or_else(|| value!(" "));
        let field_delimiter = field_delimiter.try_bytes_utf8_lossy()?;

        let whitespace = match args.optional_any("whitespace") {
            Some(whitespace) => *whitespace.downcast_ref::<Whitespace>().unwrap(),
            None => Whitespace::default(),
        };

        let standalone_key = args
            .optional("accept_standalone_key")
            .unwrap_or_else(|| value!(true))
            .try_boolean()?;

        let values = parse(
            &bytes,
            &key_value_delimiter,
            &field_delimiter,
            whitespace,
            standalone_key,
        )?;

        Ok(Value::from_iter(values))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whitespace {
    Strict,
    Lenient,
}

impl Whitespace {
    fn all_value() -> Vec<Value> {
        use Whitespace::*;

        vec![Strict, Lenient]
            .into_iter()
            .map(|u| u.as_str().into())
            .collect::<Vec<_>>()
    }

    const fn as_str(self) -> &'static str {
        use Whitespace::*;

        match self {
            Strict => "strict",
            Lenient => "lenient",
        }
    }
}

impl Default for Whitespace {
    fn default() -> Self {
        Self::Lenient
    }
}

impl FromStr for Whitespace {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Whitespace::*;

        match s {
            "strict" => Ok(Strict),
            "lenient" => Ok(Lenient),
            _ => Err("unknown whitespace variant"),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ParseKeyValueFn {
    pub(crate) value: Box<dyn Expression>,
    pub(crate) key_value_delimiter: Box<dyn Expression>,
    pub(crate) field_delimiter: Box<dyn Expression>,
    pub(crate) whitespace: Whitespace,
    pub(crate) standalone_key: Box<dyn Expression>,
}

impl Expression for ParseKeyValueFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let bytes = value.try_bytes_utf8_lossy()?;

        let value = self.key_value_delimiter.resolve(ctx)?;
        let key_value_delimiter = value.try_bytes_utf8_lossy()?;

        let value = self.field_delimiter.resolve(ctx)?;
        let field_delimiter = value.try_bytes_utf8_lossy()?;

        let standalone_key = self.standalone_key.resolve(ctx)?.try_boolean()?;

        let values = parse(
            &bytes,
            &key_value_delimiter,
            &field_delimiter,
            self.whitespace,
            standalone_key,
        )?;

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
    whitespace: Whitespace,
    standalone_key: bool,
) -> Result<Vec<(String, Value)>> {
    let (rest, result) = parse_line(
        input,
        key_value_delimiter,
        field_delimiter,
        whitespace,
        standalone_key,
    )
    .map_err(|e| match e {
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
    whitespace: Whitespace,
    standalone_key: bool,
) -> IResult<&'a str, Vec<(String, Value)>, VerboseError<&'a str>> {
    separated_list1(
        parse_field_delimiter(field_delimiter),
        parse_key_value(
            key_value_delimiter,
            field_delimiter,
            whitespace,
            standalone_key,
        ),
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
/// Always accepts `key=`
/// Accept standalone `key` if `standalone_key` is `true`
fn parse_key_value<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
    whitespace: Whitespace,
    standalone_key: bool,
) -> impl Fn(&'a str) -> IResult<&'a str, (String, Value), E> {
    move |input| {
        map(
            |input| match whitespace {
                Whitespace::Strict => tuple((
                    preceded(
                        space0,
                        parse_key(key_value_delimiter, field_delimiter, standalone_key),
                    ),
                    many_m_n(!standalone_key as usize, 1, tag(key_value_delimiter)),
                    parse_value(field_delimiter),
                ))(input),
                Whitespace::Lenient => tuple((
                    preceded(
                        space0,
                        parse_key(key_value_delimiter, field_delimiter, standalone_key),
                    ),
                    many_m_n(
                        !standalone_key as usize,
                        1,
                        delimited(space0, tag(key_value_delimiter), space0),
                    ),
                    parse_value(field_delimiter),
                ))(input),
            },
            |(field, sep, value): (&str, Vec<&str>, Value)| {
                if sep.len() == 1 {
                    (field.to_string(), value)
                } else {
                    (field.to_string(), value!(true))
                }
            },
        )(input)
    }
}

/// Parses a string delimited by the given character.
/// Can be escaped using `\`.
/// The terminator indicates the character that should follow the delimited field.
/// This captures the situation where a field is not actually delimited but starts with
/// some text that appears delimited:
/// `field: "some kind" of value`
/// We want to error in this situation rather than return a partially parsed field.
/// An error means the parser will then attempt to parse this as an undelimited field.
fn parse_delimited<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    delimiter: char,
    field_terminator: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, &'a str, E> {
    move |input| {
        terminated(
            delimited(
                char(delimiter),
                map(
                    opt(escaped(
                        recognize(many1(tuple((
                            take_while1(|c: char| c != '\\' && c != delimiter),
                            // Consume \something
                            opt(tuple((
                                satisfy(|c| c == '\\'),
                                satisfy(|c| c != '\\' && c != delimiter),
                            ))),
                        )))),
                        '\\',
                        satisfy(|c| c == '\\' || c == delimiter),
                    )),
                    |inner| inner.unwrap_or(""),
                ),
                char(delimiter),
            ),
            peek(alt((
                parse_field_delimiter(field_terminator),
                preceded(space0, eof),
            ))),
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
            alt((
                parse_delimited('"', field_delimiter),
                parse_undelimited(field_delimiter),
            )),
            Into::into,
        )(input)
    }
}

/// Parses the key.
/// Overall parsing strategies are the same as parse_value, but we don't need to convert the result to a `Value`.
/// Standalone key are handled here so a quoted standalone key that contains a delimiter will be dealt with correctly.
fn parse_key<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
    standalone_key: bool,
) -> Box<dyn Fn(&'a str) -> IResult<&'a str, &'a str, E> + 'a> {
    if standalone_key {
        Box::new(move |input| {
            alt((
                parse_delimited('"', key_value_delimiter),
                parse_delimited('"', field_delimiter),
                verify(parse_undelimited(key_value_delimiter), |s: &str| {
                    !s.contains(field_delimiter)
                }),
                parse_undelimited(field_delimiter),
            ))(input)
        })
    } else {
        Box::new(move |input| {
            alt((
                parse_delimited('"', key_value_delimiter),
                parse_undelimited(key_value_delimiter),
            ))(input)
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_quote_and_escape_char() {
        assert_eq!(
            Ok(vec![("key".to_string(), r#"a\a"#.into()),]),
            parse(r#"key="a\a""#, "=", " ", Whitespace::Strict, true,)
        );

        assert_eq!(
            Ok(vec![(r#"a\ a"#.to_string(), r#"val"#.into()),]),
            parse(r#""a\ a"=val"#, "=", " ", Whitespace::Strict, true,)
        );
    }

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
                " ",
                Whitespace::Lenient,
                false,
            )
        );
    }

    #[test]
    fn test_parse_key_value() {
        assert_eq!(
            Ok(("", ("ook".to_string(), "pook".into()))),
            parse_key_value::<VerboseError<&str>>("=", " ", Whitespace::Lenient, false)("ook=pook")
        );

        assert_eq!(
            Ok(("", ("key".to_string(), "".into()))),
            parse_key_value::<VerboseError<&str>>("=", " ", Whitespace::Strict, false)("key=")
        );
    }

    #[test]
    fn test_parse_key_values() {
        assert_eq!(
            Ok(vec![
                ("ook".to_string(), "pook".into()),
                ("onk".to_string(), "ponk".into())
            ]),
            parse("ook=pook onk=ponk", "=", " ", Whitespace::Lenient, false)
        );
    }

    #[test]
    fn test_parse_key_values_strict() {
        assert_eq!(
            Ok(vec![
                ("ook".to_string(), "".into()),
                ("onk".to_string(), "ponk".into())
            ]),
            parse("ook= onk=ponk", "=", " ", Whitespace::Strict, false)
        );
    }

    #[test]
    fn test_parse_standalone_key() {
        assert_eq!(
            Ok(vec![
                ("foo".to_string(), "bar".into()),
                ("foobar".to_string(), value!(true))
            ]),
            parse("foo:bar ,   foobar   ", ":", ",", Whitespace::Lenient, true)
        );
    }

    #[test]
    fn test_multiple_standalone_key() {
        assert_eq!(
            Ok(vec![
                ("foo".to_string(), "bar".into()),
                ("foobar".to_string(), value!(true)),
                ("bar".to_string(), "baz".into()),
                ("barfoo".to_string(), value!(true)),
            ]),
            parse(
                "foo=bar foobar bar=baz barfoo",
                "=",
                " ",
                Whitespace::Lenient,
                true
            )
        );
    }

    #[test]
    fn test_only_standalone_key() {
        assert_eq!(
            Ok(vec![
                ("foo".to_string(), value!(true)),
                ("bar".to_string(), value!(true)),
                ("foobar".to_string(), value!(true)),
                ("baz".to_string(), value!(true)),
                ("barfoo".to_string(), value!(true)),
            ]),
            parse(
                "foo bar foobar baz barfoo",
                "=",
                " ",
                Whitespace::Lenient,
                true
            )
        );
    }

    #[test]
    fn test_parse_single_standalone_key() {
        assert_eq!(
            Ok(vec![("foobar".to_string(), value!(true))]),
            parse("foobar", ":", ",", Whitespace::Lenient, true)
        );
    }

    #[test]
    fn test_parse_standalone_key_strict() {
        assert_eq!(
            Ok(vec![
                ("foo".to_string(), "bar".into()),
                ("foobar".to_string(), value!(true))
            ]),
            parse("foo:bar ,   foobar   ", ":", ",", Whitespace::Strict, true)
        );
    }

    #[test]
    fn test_parse_key() {
        // delimited
        assert_eq!(
            Ok(("", "noog")),
            parse_key::<VerboseError<&str>>("=", " ", false)(r#""noog""#)
        );

        // undelimited
        assert_eq!(
            Ok(("", "noog")),
            parse_key::<VerboseError<&str>>("=", " ", false)("noog")
        );

        // delimited with escaped char (1)
        assert_eq!(
            Ok(("=baz", r#"foo \" bar"#)),
            parse_key::<VerboseError<&str>>("=", " ", false)(r#""foo \" bar"=baz"#)
        );

        // delimited with escaped char (2)
        assert_eq!(
            Ok(("=baz", r#"foo \\ \" \ bar"#)),
            parse_key::<VerboseError<&str>>("=", " ", false)(r#""foo \\ \" \ bar"=baz"#)
        );

        // delimited with escaped char (3)
        assert_eq!(
            Ok(("=baz", r#"foo \ bar"#)),
            parse_key::<VerboseError<&str>>("=", " ", false)(r#""foo \ bar"=baz"#)
        );

        // Standalone key
        assert_eq!(
            Ok((" bar=baz", "foo")),
            parse_key::<VerboseError<&str>>("=", " ", true)(r#"foo bar=baz"#)
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

        // empty delimited
        assert_eq!(
            Ok(("", "".into())),
            parse_value::<VerboseError<&str>>(" ")(r#""""#)
        );

        // empty undelimited
        assert_eq!(
            Ok(("", "".into())),
            parse_value::<VerboseError<&str>>(" ")("")
        );
    }

    #[test]
    fn test_parse_delimited_with_internal_quotes() {
        assert!(parse_delimited::<VerboseError<&str>>('"', "=")(r#""noog" nonk"#).is_err());
    }

    #[test]
    fn test_parse_delimited_with_internal_delimiters() {
        assert_eq!(
            Ok(("", "noog nonk")),
            parse_delimited::<VerboseError<&str>>('"', " ")(r#""noog nonk""#)
        );
    }

    #[test]
    fn test_parse_undelimited_with_quotes() {
        assert_eq!(
            Ok(("", r#""noog" nonk"#)),
            parse_undelimited::<VerboseError<&str>>(":")(r#""noog" nonk"#)
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
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
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
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
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
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }

        strict {
            args: func_args! [
                value: r#"foo= bar= tar=data"#,
                whitespace: "strict"
            ],
            want: Ok(value!({foo: "",
                             bar: "",
                             tar: "data"})),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }

        spaces {
            args: func_args! [
                value: r#""zork one" : "zoog\"zink\"zork"        nonk          : nink"#,
                key_value_delimiter: ":",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             nonk: "nink"})),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }

        delimited {
            args: func_args! [
                value: r#""zork one":"zoog\"zink\"zork", nonk:nink"#,
                key_value_delimiter: ":",
                field_delimiter: ",",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             nonk: "nink"})),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }

        delimited_with_spaces {
            args: func_args! [
                value: r#""zork one" : "zoog\"zink\"zork"  ,      nonk          : nink"#,
                key_value_delimiter: ":",
                field_delimiter: ",",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             nonk: "nink"})),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }

        multiple_chars {
            args: func_args! [
                value: r#""zork one" -- "zoog\"zink\"zork"  ||    nonk          -- nink"#,
                key_value_delimiter: "--",
                field_delimiter: "||",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             nonk: "nink"})),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }

        error {
            args: func_args! [
                value: r#"I am not a valid line."#,
                key_value_delimiter: "--",
                field_delimiter: "||",
                accept_standalone_key: false,
            ],
            want: Err("0: at line 1, in Tag:\nI am not a valid line.\n                      ^\n\n1: at line 1, in ManyMN:\nI am not a valid line.\n                      ^\n\n"),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
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
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }

        // If the value field is delimited and we miss the separator,
        // the following field is consumed by the current one.
        missing_separator_delimited {
            args: func_args! [
                value: r#"zork: zoog, nonk: "nink" norgle: noog"#,
                key_value_delimiter: ":",
                field_delimiter: ",",
            ],
            want: Ok(value!({zork: "zoog",
                             nonk: r#""nink" norgle: noog"#})),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }

        multi_line_with_quotes {
            args: func_args! [
                value: "To: tom\ntest: \"tom\" test",
                key_value_delimiter: ":",
                field_delimiter: "\n",
            ],
            want: Ok(value!({"To": "tom",
                             "test": "\"tom\" test"})),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }

        multi_line_with_quotes_spaces {
            args: func_args! [
                value: "To: tom\ntest: \"tom test\"  ",
                key_value_delimiter: ":",
                field_delimiter: "\n",
            ],
            want: Ok(value!({"To": "tom",
                             "test": "tom test"})),
            tdef: TypeDef::new().fallible().object::<(), Kind>(map! {
                (): Kind::all()
            }),
        }
    ];
}
