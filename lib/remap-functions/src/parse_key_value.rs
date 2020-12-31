use nom::{
    self, branch::alt, bytes::complete::tag, bytes::complete::take_until, combinator::map,
    combinator::rest, multi::separated_list1, sequence::tuple, IResult,
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
                keyword: "field_split",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
            Parameter {
                keyword: "separator",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
            Parameter {
                keyword: "trim_key",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
            Parameter {
                keyword: "trim_value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let field_split = arguments.optional("field_split").map(Expr::boxed);
        let separator = arguments.optional("separator").map(Expr::boxed);
        let trim_key = arguments.optional("trim_key").map(Expr::boxed);
        let trim_value = arguments.optional("trim_value").map(Expr::boxed);

        Ok(Box::new(ParseKeyValueFn {
            value,
            field_split,
            separator,
            trim_key,
            trim_value,
        }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParseKeyValueFn {
    value: Box<dyn Expression>,
    field_split: Option<Box<dyn Expression>>,
    separator: Option<Box<dyn Expression>>,
    trim_key: Option<Box<dyn Expression>>,
    trim_value: Option<Box<dyn Expression>>,
}

fn parse_pair(
    pair: &str,
    field_split: &str,
    trim_key: &Option<Vec<char>>,
    trim_value: &Option<Vec<char>>,
) -> Option<(String, Value)> {
    let pair = pair.trim();

    let split_index = pair.find(field_split).unwrap_or(0);
    let (key, _val) = pair.split_at(split_index);
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let key = match trim_key {
        Some(trim_key) => key.trim_matches(trim_key as &[_]),
        None => key,
    };

    let val = pair[split_index + field_split.len()..].trim();
    let val = match trim_value {
        Some(trim_value) => val.trim_matches(trim_value as &[_]),
        None => val,
    };

    Some((key.to_string(), val.into()))
}

impl Expression for ParseKeyValueFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        let field_split = match &self.field_split {
            Some(s) => String::from_utf8_lossy(&s.execute(state, object)?.try_bytes()?).to_string(),
            None => "=".to_string(),
        };

        let separator = match &self.separator {
            Some(s) => String::from_utf8_lossy(&s.execute(state, object)?.try_bytes()?).to_string(),
            None => " ".to_string(),
        };

        let trim_key = match &self.trim_key {
            Some(s) => Some(
                String::from_utf8_lossy(&s.execute(state, object)?.try_bytes()?)
                    .chars()
                    .collect::<Vec<_>>(),
            ),
            None => None,
        };

        let trim_value = match &self.trim_value {
            Some(s) => Some(
                String::from_utf8_lossy(&s.execute(state, object)?.try_bytes()?)
                    .chars()
                    .collect::<Vec<_>>(),
            ),
            None => None,
        };

        /*
        Ok(value
            .split(&separator)
            .filter_map(|pair| parse_pair(pair, &field_split, &trim_key, &trim_value))
            .collect::<BTreeMap<_, _>>()
            .into())
        */

        let (_, values) =
            parse_line(&value, &field_split, &separator).map_err(|e| e.to_string())?;

        Ok(values.into_iter().collect::<BTreeMap<_, _>>().into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .merge_optional(self.field_split.as_ref().map(|field_split| {
                field_split
                    .type_def(state)
                    .fallible_unless(value::Kind::Bytes)
            }))
            .merge_optional(self.separator.as_ref().map(|separator| {
                separator
                    .type_def(state)
                    .fallible_unless(value::Kind::Bytes)
            }))
            .merge_optional(
                self.trim_key
                    .as_ref()
                    .map(|trim_key| trim_key.type_def(state).fallible_unless(value::Kind::Bytes)),
            )
            .merge_optional(self.trim_value.as_ref().map(|trim_value| {
                trim_value
                    .type_def(state)
                    .fallible_unless(value::Kind::Bytes)
            }))
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Map)
    }
}

fn parse_line<'a>(
    input: &'a str,
    field_split: &'a str,
    separator: &'a str,
) -> IResult<&'a str, Vec<(String, Value)>> {
    separated_list1(tag(separator), parse_key_value(field_split, separator))(input)
}

/// Parse a single `key=value` tuple.
fn parse_key_value<'a>(
    field_split: &'a str,
    separator: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, (String, Value)> {
    move |input| {
        map(
            tuple((
                take_until(field_split),
                tag(field_split),
                alt((take_until(separator), rest)),
            )),
            |(field, _, value): (&str, &str, &str)| (field.to_string(), value.into()),
        )(input)
    }
}

/// Checks if the current input starts with a delimiter character.
/// If it does it returns the end delimiter. Otherwise it returns None.
fn is_delimited(input: &str, delimiters: &Option<String>) -> Option<char> {
    match (input.chars().next(), delimiters) {
        (Some(chr), Some(delimiters)) => {
            let mut delimiters = delimiters.chars();
            match delimiters.next() {
                Some(start) if chr == start => match delimiters.next() {
                    Some(next) => Some(next),
                    None => Some(start),
                },
                _ => None,
            }
        }
        (_, _) => None,
    }
}

/// Parses the value.
/// The value has two parsing strategies.
///
/// 1. If it starts with one of the trim values, we assume it is a delimited field, so we parse up to
///    the closing delimiter. If the trim_value is one character, this is the close, if it is more than
///    one, than the close is any character other than the opening delimiter.
/// 2. If it does not start with one of the trim values, it is not a delimited field and we parse up to
///    the next separator or the eof.
///
fn parse_value<'a>(
    trim_value: &'a Option<String>,
    separator: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, Value> {
    move |input| {
        map(
            match is_delimited(input, trim_value) {
                Some(_) => alt((take_until(separator), rest)),
                None => alt((take_until(separator), rest)),
            },
            |value| Value::from(value),
        )(input)
    }
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
                    ("nork".to_string(), "noog".into())
                ]
            )),
            parse_line("ook=pook nork=noog", "=", " ")
        );
    }

    #[test]
    fn test_parse_value() {
        assert_eq!(
            Ok(("", "noog".into())),
            parse_value(&Some(r#"""#.to_string()), " ")(r#""noog""#)
        );
    }

    test_type_def![
        value_string {
            expr: |_| ParseKeyValueFn {
                value: Literal::from("foo").boxed(),
                field_split: None,
                separator: None,
                trim_key: None,
                trim_value: None,
            },
            def: TypeDef {
                kind: Kind::Map,
                ..Default::default()
            },
        }

        value_non_string {
            expr: |_| ParseKeyValueFn {
                value: Literal::from(1).boxed(),
                field_split: None,
                separator: None,
                trim_key: None,
                trim_value: None,
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
                field_split: Some(Literal::from("=").boxed()),
                separator: None,
                trim_key: None,
                trim_value: None,
            },
            def: TypeDef {
                kind: Kind::Map,
                ..Default::default()
            },
        }

        optional_value_non_string {
            expr: |_| ParseKeyValueFn {
                value: Literal::from("ook").boxed(),
                field_split: Some(Literal::from(1).boxed()),
                separator: None,
                trim_key: None,
                trim_value: None,
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
                value: "at=info method=GET path=/ host=myapp.herokuapp.com request_id=8601b555-6a83-4c12-8269-97c8e32cdb22 fwd=\"204.204.204.204\" dyno=web.1 connect=1ms service=18ms status=200 bytes=13 tls_version=tls1.1 protocol=http",
            ],
            want: Ok(value!({"at": "info",
                             "method": "GET",
                             "path": "/",
                             "host": "myapp.herokuapp.com",
                             "request_id": "8601b555-6a83-4c12-8269-97c8e32cdb22",
                             "fwd": "\"204.204.204.204\"",
                             "dyno": "web.1",
                             "connect": "1ms",
                             "service": "18ms",
                             "status": "200",
                             "bytes": "13",
                             "tls_version": "tls1.1",
                             "protocol": "http"}))
        }

        custom_separator {
            args: func_args! [
                value: "'zork': <zoog>, 'nonk': <nink>",
                field_split: ":",
                separator: ",",
                trim_key: "'",
                trim_value: "<>"
            ],
            want: Ok(value!({"zork": "zoog",
                             "nonk": "nink"}))
        }
    ];
}
