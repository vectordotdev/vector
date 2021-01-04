use nom::{
    self,
    branch::alt,
    bytes::complete::{escaped, tag, take_until, take_while1},
    character::complete::{char, satisfy, space0},
    combinator::{map, rest},
    multi::{many1, separated_list1},
    sequence::{delimited, preceded, tuple},
    IResult,
};
use remap::prelude::*;
use std::collections::BTreeMap;

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
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "key_value_delimiter",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
            Parameter {
                keyword: "field_delimiter",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let key_value_delimiter = arguments.optional("key_value_delimiter").map(Expr::boxed);
        let field_delimiter = arguments.optional("field_delimiter").map(Expr::boxed);

        Ok(Box::new(ParseKeyValueFn {
            value,
            key_value_delimiter,
            field_delimiter,
        }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParseKeyValueFn {
    value: Box<dyn Expression>,
    key_value_delimiter: Option<Box<dyn Expression>>,
    field_delimiter: Option<Box<dyn Expression>>,
}

impl Expression for ParseKeyValueFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        let key_value_delimiter = match &self.key_value_delimiter {
            Some(s) => String::from_utf8_lossy(&s.execute(state, object)?.try_bytes()?).to_string(),
            None => "=".to_string(),
        };

        let field_delimiter = match &self.field_delimiter {
            Some(s) => String::from_utf8_lossy(&s.execute(state, object)?.try_bytes()?).to_string(),
            None => " ".to_string(),
        };

        let (_, values) =
            parse_line(&value, &key_value_delimiter, &field_delimiter).map_err(|e| e.to_string())?;

        Ok(values.into_iter().collect::<BTreeMap<_, _>>().into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .merge_optional(
                self.key_value_delimiter
                    .as_ref()
                    .map(|key_value_delimiter| key_value_delimiter.type_def(state)),
            )
            .merge_optional(
                self.field_delimiter
                    .as_ref()
                    .map(|field_delimiter| field_delimiter.type_def(state)),
            )
            .into_fallible(true)
            .with_constraint(value::Kind::Map)
    }
}

/// Parse the line as a separated list of key value pairs.
fn parse_line<'a>(
    input: &'a str,
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
) -> IResult<&'a str, Vec<(String, Value)>> {
    separated_list1(
        parse_field_delimiter(field_delimiter),
        parse_key_value(key_value_delimiter, field_delimiter),
    )(input)
}

/// Parses the field_delimiter between the key/value pairs.
/// If the field_delimiter is a space, we parse as many as we can,
/// If it is not a space eat any whitespace before our field_delimiter as well as the field_delimiter.
/// These lifetimes are actually needed.
#[allow(clippy::needless_lifetimes)]
fn parse_field_delimiter<'a>(field_delimiter: &'a str) -> impl Fn(&'a str) -> IResult<&str, &str> {
    move |input| {
        if field_delimiter == " " {
            map(many1(tag(field_delimiter)), |_| " ")(input)
        } else {
            preceded(space0, tag(field_delimiter))(input)
        }
    }
}

/// Parse a single `key=value` tuple.
/// These lifetimes are actually needed.
#[allow(clippy::needless_lifetimes)]
fn parse_key_value<'a>(
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, (String, Value)> {
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
fn parse_delimited(delimiter: char) -> impl Fn(&str) -> IResult<&str, &str> {
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
/// These lifetimes are actually needed.
#[allow(clippy::needless_lifetimes)]
fn parse_undelimited<'a>(field_delimiter: &'a str) -> impl Fn(&'a str) -> IResult<&str, &str> {
    move |input| map(alt((take_until(field_delimiter), rest)), |s: &str| s.trim())(input)
}

/// Parses the value.
/// The value has two parsing strategies.
///
/// 1. Parse as a delimited field - currently the delimiter is hardcoded to a `"`.
/// 2. If it does not start with one of the trim values, it is not a delimited field and we parse up to
///    the next field_delimiter or the eof.
///
/// These lifetimes are actually needed.
#[allow(clippy::needless_lifetimes)]
fn parse_value<'a>(field_delimiter: &'a str) -> impl Fn(&'a str) -> IResult<&str, Value> {
    move |input| {
        map(
            alt((parse_delimited('"'), parse_undelimited(field_delimiter))),
            Into::into,
        )(input)
    }
}

/// Parses the key.
/// Parsing strategies are the same as parse_value, but we don't need to convert the result to a `Value`.
/// These lifetimes are actually needed.
#[allow(clippy::needless_lifetimes)]
fn parse_key<'a>(field_delimiter: &'a str) -> impl Fn(&'a str) -> IResult<&str, &str> {
    move |input| alt((parse_delimited('"'), parse_undelimited(field_delimiter)))(input)
}

#[cfg(test)]
mod test {
    use super::*;
    use remap::value;
    use value::Kind;

    #[test]
    fn test_parse() {
        assert_eq!(
            Ok(("", ("ook".to_string(), "pook".into()))),
            parse_key_value("=", " ")("ook=pook")
        );
    }

    #[test]
    fn test_parse_line() {
        assert_eq!(
            Ok((
                "",
                vec![
                    ("ook".to_string(), "pook".into()),
                    (
                        "@timestamp".to_string(),
                        "2020-12-31T12:43:22.2322232Z".into()
                    ),
                    ("key#hash".to_string(), "value".into()),
                    ("key=with=special=characters".to_string(), "value".into()),
                    ("key".to_string(), "with special=characters".into()),
                ]
            )),
            parse_line(
                r#"ook=pook @timestamp=2020-12-31T12:43:22.2322232Z key#hash=value "key=with=special=characters"=value key="with special=characters""#,
                "=",
                " "
            )
        );
    }

    #[test]
    fn test_parse_value() {
        // delimited
        assert_eq!(Ok(("", "noog".into())), parse_value(" ")(r#""noog""#));

        // undelimited
        assert_eq!(Ok(("", "noog".into())), parse_value(" ")("noog"));
    }

    test_type_def![
        value_string {
            expr: |_| ParseKeyValueFn {
                value: Literal::from("foo").boxed(),
                key_value_delimiter: None,
                field_delimiter: None,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Map,
                ..Default::default()
            },
        }

        value_non_string {
            expr: |_| ParseKeyValueFn {
                value: Literal::from(1).boxed(),
                key_value_delimiter: None,
                field_delimiter: None,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Map,
                ..Default::default()
            },
        }

        optional_value_string {
            expr: |_| ParseKeyValueFn {
                value: Literal::from("ook").boxed(),
                key_value_delimiter: Some(Literal::from("=").boxed()),
                field_delimiter: None,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Map,
                ..Default::default()
            },
        }

        optional_value_non_string {
            expr: |_| ParseKeyValueFn {
                value: Literal::from("ook").boxed(),
                key_value_delimiter: Some(Literal::from(1).boxed()),
                field_delimiter: None,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Map,
                ..Default::default()
            },
        }
    ];

    test_function![
        parse_key_value => ParseKeyValue;

        default {
            args: func_args! [
                value: r#"at=info method=GET path=/ host=myapp.herokuapp.com request_id=8601b555-6a83-4c12-8269-97c8e32cdb22 fwd="204.204.204.204" dyno=web.1 connect=1ms service=18ms status=200 bytes=13 tls_version=tls1.1 protocol=http"#,
            ],
            want: Ok(value!({"at": "info",
                             "method": "GET",
                             "path": "/",
                             "host": "myapp.herokuapp.com",
                             "request_id": "8601b555-6a83-4c12-8269-97c8e32cdb22",
                             "fwd": "204.204.204.204",
                             "dyno": "web.1",
                             "connect": "1ms",
                             "service": "18ms",
                             "status": "200",
                             "bytes": "13",
                             "tls_version": "tls1.1",
                             "protocol": "http"}))
        }

        logfmt {
            args: func_args! [
                value: r#"level=info msg="Stopping all fetchers" tag=stopping_fetchers id=ConsumerFetcherManager-1382721708341 module=kafka.consumer.ConsumerFetcherManager"#
            ],
            want: Ok(value!({"level": "info",
                             "msg": "Stopping all fetchers",
                             "tag": "stopping_fetchers",
                             "id": "ConsumerFetcherManager-1382721708341",
                             "module": "kafka.consumer.ConsumerFetcherManager"}))
        }

        spaces {
            args: func_args! [
                value: r#""zork one" : "zoog\"zink\"zork"        nonk          : nink"#,
                key_value_delimiter: ":",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             "nonk": "nink"}))
        }

        delimited {
            args: func_args! [
                value: r#""zork one":"zoog\"zink\"zork", nonk:nink"#,
                key_value_delimiter: ":",
                field_delimiter: ",",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             "nonk": "nink"}))
        }

        delimited_with_spaces {
            args: func_args! [
                value: r#""zork one" : "zoog\"zink\"zork"  ,      nonk          : nink"#,
                key_value_delimiter: ":",
                field_delimiter: ",",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             "nonk": "nink"}))
        }

        multiple_chars {
            args: func_args! [
                value: r#""zork one" -- "zoog\"zink\"zork"  ||    nonk          -- nink"#,
                key_value_delimiter: "--",
                field_delimiter: "||",
            ],
            want: Ok(value!({"zork one": r#"zoog\"zink\"zork"#,
                             "nonk": "nink"}))
        }
    ];
}
