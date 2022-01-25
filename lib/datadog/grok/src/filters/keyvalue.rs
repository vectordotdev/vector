use std::collections::BTreeMap;
use std::fmt::Formatter;

use bytes::Bytes;
use lookup::{Lookup, LookupBuf};
use nom::{
    self,
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, digit1, space0, space1},
    combinator::{eof, map, opt, peek, rest, value},
    multi::{many_m_n, separated_list1},
    number::complete::double,
    sequence::{delimited, preceded, terminated, tuple},
    IResult,
};
use nom_regex::str::re_find;
use ordered_float::NotNan;
use regex::Regex;
use vrl_compiler::{Target, Value};

use crate::{
    ast::{Function, FunctionArgument},
    grok_filter::GrokFilter,
    parse_grok::Error as GrokRuntimeError,
    parse_grok_rules::Error as GrokStaticError,
};

pub fn filter_from_function(f: &Function) -> Result<GrokFilter, GrokStaticError> {
    {
        let args_len = f.args.as_ref().map_or(0, |args| args.len());

        let key_value_delimiter = if args_len > 0 {
            match f.args.as_ref().unwrap()[0] {
                FunctionArgument::Arg(Value::Bytes(ref bytes)) => {
                    String::from_utf8_lossy(bytes).to_string()
                }
                _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
            }
        } else {
            // default key/value delimiter
            "=".to_string()
        };
        let value_re = if args_len > 1 {
            match f.args.as_ref().unwrap()[1] {
                FunctionArgument::Arg(Value::Bytes(ref bytes)) => {
                    let mut re_str = String::new();
                    re_str.push_str(r"^[\w.\-_@");
                    re_str.push_str(&String::from_utf8_lossy(bytes).to_string());
                    re_str.push_str("]+");
                    Regex::new(re_str.as_str())
                        .map_err(|_e| GrokStaticError::InvalidFunctionArguments(f.name.clone()))?
                }
                _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
            }
        } else {
            // default allowed unescaped symbols
            Regex::new(r"^[\w.\-_@]*").unwrap()
        };

        let quotes = if args_len > 2 {
            match f.args.as_ref().unwrap()[2] {
                FunctionArgument::Arg(Value::Bytes(ref bytes)) => {
                    let quotes = String::from_utf8_lossy(bytes);
                    if quotes.len() == 2 {
                        let mut chars = quotes.chars();
                        vec![(
                            chars.next().expect("open quote"),
                            chars.next().expect("closing quote"),
                        )]
                    } else if quotes.is_empty() {
                        // default
                        vec![('"', '"'), ('\'', '\''), ('<', '>')]
                    } else {
                        return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone()));
                    }
                }
                _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
            }
        } else {
            // default quotes
            vec![('"', '"'), ('\'', '\''), ('<', '>')]
        };

        let field_delimiters = if args_len > 3 {
            match f.args.as_ref().unwrap()[3] {
                FunctionArgument::Arg(Value::Bytes(ref bytes)) => {
                    vec![String::from_utf8_lossy(bytes).to_string()]
                }
                _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
            }
        } else {
            // default field delimiters
            vec![" ".into(), ",".into(), ";".into()]
        };
        Ok(GrokFilter::KeyValue(KeyValueFilter {
            key_value_delimiter,
            value_re,
            quotes,
            field_delimiters,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct KeyValueFilter {
    pub key_value_delimiter: String,
    pub value_re: Regex,
    pub quotes: Vec<(char, char)>,
    pub field_delimiters: Vec<String>,
}

impl std::fmt::Display for KeyValueFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "keyvalue(\"{}\", \"{}\", \"{:?}\", \"{:?}\")",
            self.key_value_delimiter, self.value_re, self.quotes, self.field_delimiters,
        )
    }
}

pub fn apply_filter(value: &Value, filter: &KeyValueFilter) -> Result<Value, GrokRuntimeError> {
    match value {
        Value::Bytes(bytes) => {
            let mut result = Value::Object(BTreeMap::default());
            parse(
                String::from_utf8_lossy(bytes).as_ref(),
                &filter.key_value_delimiter,
                &filter
                    .field_delimiters
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<&str>>(),
                &filter.quotes,
                &filter.value_re,
            )
            .unwrap_or_default()
            .into_iter()
            .for_each(|(k, v)| {
                if !(v.is_null()
                    || matches!(&v, Value::Bytes(b) if b.is_empty())
                    || k.trim().is_empty())
                {
                    let lookup: LookupBuf = Lookup::from_str(&k)
                        .unwrap_or_else(|_| Lookup::from(&k))
                        .into();
                    result.insert(&lookup, v).unwrap_or_else(
                        |error| warn!(message = "Error updating field value", field = %lookup, %error)
                    );
                }
            });
            Ok(result)
        }
        _ => Err(GrokRuntimeError::FailedToApplyFilter(
            filter.to_string(),
            value.to_string(),
        )),
    }
}

type SResult<'a, O> = IResult<&'a str, O, (&'a str, nom::error::ErrorKind)>;

fn parse<'a>(
    input: &'a str,
    key_value_delimiter: &'a str,
    field_delimiters: &'a [&'a str],
    quotes: &'a [(char, char)],
    value_re: &Regex,
) -> Result<Vec<(String, Value)>, String> {
    let (rest, result) = parse_line(
        input,
        key_value_delimiter,
        field_delimiters,
        quotes,
        value_re,
    )
    .map_err(|_| format!("could not parse '{}' as 'keyvalue'", input))?;

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
    field_delimiters: &'a [&'a str],
    quotes: &'a [(char, char)],
    value_re: &'a Regex,
) -> SResult<'a, Vec<(String, Value)>> {
    let mut last_result = None;
    for &field_delimiter in field_delimiters {
        match separated_list1(
            parse_field_delimiter(field_delimiter),
            parse_key_value(key_value_delimiter, field_delimiter, quotes, value_re),
        )(input)
        {
            Ok((rest, v)) if rest.trim().is_empty() => {
                return Ok((rest, v));
            }
            res => last_result = Some(res), // continue
        }
    }
    last_result.unwrap()
}

/// Parses the field_delimiter between the key/value pairs, ignoring surrounding spaces
fn parse_field_delimiter<'a>(field_delimiter: &'a str) -> impl Fn(&'a str) -> SResult<&'a str> {
    move |input| {
        if field_delimiter == " " {
            space1(input)
        } else {
            preceded(space0, tag(field_delimiter))(input)
        }
    }
}

/// Parses the end of input ignoring any trailing spaces.
fn parse_end_of_input<'a>() -> impl Fn(&'a str) -> SResult<&'a str> {
    move |input| preceded(space0, eof)(input)
}

/// Parse a single `key=value` tuple.
/// Does not accept standalone keys(`key=`)
fn parse_key_value<'a>(
    key_value_delimiter: &'a str,
    field_delimiter: &'a str,
    quotes: &'a [(char, char)],
    non_quoted_re: &'a Regex,
) -> impl Fn(&'a str) -> SResult<(String, Value)> {
    move |input| {
        map(
            |input| {
                tuple((
                    alt((
                        preceded(
                            space0,
                            parse_key(key_value_delimiter, quotes, non_quoted_re),
                        ),
                        preceded(space0, parse_key(field_delimiter, quotes, non_quoted_re)),
                    )),
                    many_m_n(0, 1, tag(key_value_delimiter)),
                    parse_value(field_delimiter, quotes, non_quoted_re),
                ))(input)
            },
            |(field, sep, value): (&str, Vec<&str>, Value)| {
                if sep.len() == 1 {
                    (field.to_string(), value)
                } else {
                    (field.to_string(), Value::Null) // will be removed
                }
            },
        )(input)
    }
}

/// Parses quoted strings.
fn parse_quoted<'a>(
    quotes: &'a (char, char),
    field_terminator: &'a str,
) -> impl Fn(&'a str) -> SResult<&'a str> {
    move |input| {
        terminated(
            delimited(
                char(quotes.0),
                map(opt(take_while1(|c: char| c != quotes.1)), |inner| {
                    inner.unwrap_or("")
                }),
                char(quotes.1),
            ),
            peek(alt((
                parse_field_delimiter(field_terminator),
                parse_end_of_input(),
            ))),
        )(input)
    }
}

/// A delimited value is all the text until our field_delimiter, or the rest of the string if it is the last value in the line,
fn parse_delimited<'a>(field_delimiter: &'a str) -> impl Fn(&'a str) -> SResult<&'a str> {
    move |input| map(alt((take_until(field_delimiter), rest)), |s: &str| s.trim())(input)
}

fn quoted<'a>(
    quotes: &'a [(char, char)],
    delimiter: &'a str,
) -> impl Fn(&'a str) -> SResult<&'a str> {
    move |input| {
        let mut last_err = None;
        for quotes in quotes {
            match parse_quoted(quotes, delimiter)(input) {
                done @ Ok(..) => return done,
                err @ Err(..) => last_err = Some(err), // continue
            }
        }
        last_err.unwrap()
    }
}

/// Parses an input while it matches a given regex, otherwise skips an input until the next field delimiter
fn match_re_or_empty<'a>(
    value_re: &'a Regex,
    field_delimiter: &'a str,
) -> impl Fn(&'a str) -> SResult<&'a str> {
    move |input| {
        re_find::<'a, (&'a str, nom::error::ErrorKind)>(value_re.clone())(input)
            .or_else(|_| parse_delimited(field_delimiter)(input).map(|(rest, _v)| (rest, "")))
    }
}

/// Parses the value.
/// The value has two parsing strategies.
///
/// 1. The value is quoted - parse until the end quote
/// 2. Otherwise we parse until regex matches
fn parse_value<'a>(
    field_delimiter: &'a str,
    quotes: &'a [(char, char)],
    re: &'a Regex,
) -> impl Fn(&'a str) -> SResult<Value> {
    move |input| {
        alt((
            map(quoted(quotes, field_delimiter), |value| {
                Value::Bytes(Bytes::copy_from_slice(value.as_bytes()))
            }),
            parse_null,
            parse_boolean,
            map(
                terminated(
                    digit1,
                    peek(alt((
                        parse_field_delimiter(field_delimiter),
                        parse_end_of_input(),
                    ))),
                ),
                |v: &'a str| Value::Integer(v.parse().expect("not an integer")),
            ),
            map(double, |value| {
                Value::Float(NotNan::new(value).expect("not a float"))
            }),
            map(match_re_or_empty(re, field_delimiter), |value| {
                Value::Bytes(Bytes::copy_from_slice(value.as_bytes()))
            }),
        ))(input)
    }
}

fn parse_null(input: &str) -> SResult<Value> {
    value(Value::Null, tag("null"))(input)
}

fn parse_boolean(input: &str) -> SResult<Value> {
    let parse_true = value(Value::Boolean(true), tag("true"));
    let parse_false = value(Value::Boolean(false), tag("false"));

    alt((parse_true, parse_false))(input)
}

/// Parses the key.
/// Parsing strategies are the same as parse_value, but we don't need to convert the result to a `Value`.
fn parse_key<'a>(
    key_value_delimiter: &'a str,
    quotes: &'a [(char, char)],
    re: &'a Regex,
) -> impl Fn(&'a str) -> SResult<&'a str> {
    move |input| alt((quoted(quotes, key_value_delimiter), re_find(re.to_owned())))(input)
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;

    #[test]
    fn test_parse_keyvalue() {
        // DD examples from https://docs.datadoghq.com/logs/log_configuration/parsing/?tab=filters#key-value-or-logfmt
        let default_value_re = Regex::new(r"^[\w.\-_@]+").unwrap();
        let default_quotes = &[('"', '"'), ('\'', '\''), ('<', '>')];

        let default_key_value_delimiter = "=";
        let default_field_delimiters = &[" ", ",", ";"];

        assert_eq!(
            Ok(vec![("key".to_string(), "valueStr".into()),]),
            parse(
                "key=valueStr",
                default_key_value_delimiter,
                default_field_delimiters,
                default_quotes,
                &default_value_re,
            )
        );
        assert_eq!(
            Ok(vec![("key".to_string(), "valueStr".into()),]),
            parse(
                "key=<valueStr>",
                default_key_value_delimiter,
                default_field_delimiters,
                default_quotes,
                &default_value_re,
            )
        );
        assert_eq!(
            Ok(vec![("key".to_string(), "valueStr".into()),]),
            parse(
                r#""key"="valueStr""#,
                default_key_value_delimiter,
                default_field_delimiters,
                default_quotes,
                &default_value_re,
            )
        );
        assert_eq!(
            Ok(vec![("key".to_string(), "valueStr".into()),]),
            parse(
                "key:valueStr",
                ":",
                default_field_delimiters,
                default_quotes,
                &default_value_re,
            )
        );
        assert_eq!(
            Ok(vec![("key".to_string(), "/valueStr".into()),]),
            parse(
                r#"key:"/valueStr""#,
                ":",
                default_field_delimiters,
                default_quotes,
                &default_value_re,
            )
        );
        assert_eq!(
            Ok(vec![("/key".to_string(), "/valueStr".into()),]),
            parse(
                r#"/key:/valueStr"#,
                ":",
                default_field_delimiters,
                default_quotes,
                &Regex::new(r"^[\w.\-_@/]+").unwrap(),
            )
        );
        assert_eq!(
            Ok(vec![("key".to_string(), "valueStr".into()),]),
            parse(
                r#"key:={valueStr}"#,
                ":=",
                default_field_delimiters,
                &[('{', '}')],
                &default_value_re,
            )
        );
        assert_eq!(
            Ok(vec![
                ("key1".to_string(), "value1".into()),
                ("key2".to_string(), "value2".into())
            ]),
            parse(
                r#"key1=value1|key2=value2"#,
                default_key_value_delimiter,
                &["|"],
                default_quotes,
                &default_value_re,
            )
        );
        assert_eq!(
            Ok(vec![
                ("key1".to_string(), "value1".into()),
                ("key2".to_string(), "value2".into())
            ]),
            parse(
                r#"key1="value1"|key2="value2""#,
                default_key_value_delimiter,
                &["|"],
                default_quotes,
                &default_value_re,
            )
        );
    }
}
