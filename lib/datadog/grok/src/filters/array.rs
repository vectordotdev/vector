use std::convert::TryFrom;

use bytes::Bytes;
use nom::{
    branch::alt,
    bytes::complete::{tag, take, take_until},
    character::complete::char,
    combinator::{cut, map},
    multi::separated_list0,
    sequence::{preceded, terminated},
    IResult,
};
use vrl_compiler::Value;

use crate::{
    ast::{Function, FunctionArgument},
    grok_filter::GrokFilter,
    parse_grok_rules::Error as GrokStaticError,
};

pub fn filter_from_function(f: &Function) -> Result<GrokFilter, GrokStaticError> {
    let args = f.args.as_ref();
    let args_len = args.map_or(0, |args| args.len());

    let mut delimiter = None;
    let mut value_filter = None;
    let mut brackets = None;
    if args_len == 1 {
        match &args.unwrap()[0] {
            FunctionArgument::Arg(Value::Bytes(ref bytes)) => {
                delimiter = Some(String::from_utf8_lossy(bytes).to_string());
            }
            FunctionArgument::Function(f) => value_filter = Some(GrokFilter::try_from(f)?),
            _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
        }
    } else if args_len == 2 {
        match (&args.unwrap()[0], &args.unwrap()[1]) {
            (
                FunctionArgument::Arg(Value::Bytes(ref brackets_b)),
                FunctionArgument::Arg(Value::Bytes(ref delimiter_b)),
            ) => {
                brackets = Some(String::from_utf8_lossy(brackets_b).to_string());
                delimiter = Some(String::from_utf8_lossy(delimiter_b).to_string());
            }
            (
                FunctionArgument::Arg(Value::Bytes(ref delimiter_b)),
                FunctionArgument::Function(f),
            ) => {
                delimiter = Some(String::from_utf8_lossy(delimiter_b).to_string());
                value_filter = Some(GrokFilter::try_from(f)?);
            }
            _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
        }
    } else if args_len == 3 {
        match (&args.unwrap()[0], &args.unwrap()[1], &args.unwrap()[2]) {
            (
                FunctionArgument::Arg(Value::Bytes(ref brackets_b)),
                FunctionArgument::Arg(Value::Bytes(ref delimiter_b)),
                FunctionArgument::Function(f),
            ) => {
                brackets = Some(String::from_utf8_lossy(brackets_b).to_string());
                delimiter = Some(String::from_utf8_lossy(delimiter_b).to_string());
                value_filter = Some(GrokFilter::try_from(f)?);
            }
            _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
        }
    } else if args_len > 3 {
        return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone()));
    }

    let brackets = match brackets {
        Some(b) if b.len() == 1 => {
            let char = b.chars().next().unwrap();
            Some((char, char))
        }
        Some(b) if b.len() == 2 => {
            let mut chars = b.chars();
            Some((chars.next().unwrap(), chars.next().unwrap()))
        }
        None => None,
        _ => {
            return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone()));
        }
    };

    Ok(GrokFilter::Array(
        brackets,
        delimiter,
        Box::new(value_filter),
    ))
}

type SResult<'a, O> = IResult<&'a str, O, (&'a str, nom::error::ErrorKind)>;

pub fn parse<'a>(
    input: &'a str,
    brackets: Option<(char, char)>,
    delimiter: Option<&'a str>,
) -> Result<Vec<Value>, String> {
    let result = parse_array(brackets, delimiter)(input)
        .map_err(|_| format!("could not parse '{}' as array", input))
        .and_then(|(rest, result)| {
            rest.trim()
                .is_empty()
                .then(|| result)
                .ok_or_else(|| format!("could not parse '{}' as array", input))
        })?;

    Ok(result)
}

fn parse_array<'a>(
    brackets: Option<(char, char)>,
    delimiter: Option<&'a str>,
) -> impl Fn(&'a str) -> SResult<Vec<Value>> {
    let brackets = brackets.unwrap_or(('[', ']'));
    move |input| {
        preceded(
            char(brackets.0),
            terminated(cut(parse_array_values(delimiter)), char(brackets.1)),
        )(input)
    }
}

fn parse_array_values<'a>(delimiter: Option<&'a str>) -> impl Fn(&'a str) -> SResult<Vec<Value>> {
    move |input| {
        let delimiter = delimiter.unwrap_or(",");
        // skip the last closing character
        separated_list0(tag(delimiter), cut(parse_value(delimiter)))(&input[..input.len() - 1]).map(
            |(rest, values)| {
                if rest.is_empty() {
                    // return the closing character
                    (&input[input.len() - 1..], values)
                } else {
                    (rest, values) // will fail upstream
                }
            },
        )
    }
}

fn parse_value<'a>(delimiter: &'a str) -> impl Fn(&'a str) -> SResult<Value> {
    move |input| {
        map(
            alt((take_until(delimiter), take(input.len()))),
            |value: &str| Value::Bytes(Bytes::copy_from_slice(value.as_bytes())),
        )(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default() {
        let result = parse(r#"[ 1 ,2]"#, None, None).unwrap();
        assert_eq!(result, vec![" 1 ".into(), "2".into()]);
    }

    #[test]
    fn parses_with_non_default_brackets() {
        let result = parse("{1,2}", Some(('{', '}')), None).unwrap();
        assert_eq!(result, vec!["1".into(), "2".into()]);
    }

    #[test]
    fn parses_quotes() {
        let result = parse(r#"["1,2"]"#, None, None).unwrap();
        assert_eq!(result, vec!["\"1".into(), "2\"".into()]);
    }

    #[test]
    fn parses_escaped_special_characters() {
        let result = parse("[1\r2]", None, Some("\r")).unwrap();
        assert_eq!(result, vec!["1".into(), "2".into()]);

        let result = parse("[1\n2]", None, Some("\n")).unwrap();
        assert_eq!(result, vec!["1".into(), "2".into()]);

        let result = parse("[1\t2]", None, Some("\t")).unwrap();
        assert_eq!(result, vec!["1".into(), "2".into()]);
    }
}
