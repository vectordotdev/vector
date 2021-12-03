use crate::{
    ast::{Function, FunctionArgument},
    parse_grok::Error as GrokRuntimeError,
    parse_grok_rules::Error as GrokStaticError,
};
use bytes::Bytes;
use nom::{
    branch::alt,
    bytes::complete::{tag, take, take_until},
    character::complete::char,
    combinator::cut,
    combinator::map,
    error::{ContextError, ParseError},
    multi::separated_list0,
    sequence::{preceded, terminated},
    IResult,
};
use std::convert::TryFrom;

use crate::grok_filter::GrokFilter;
use vrl_compiler::Value;

pub fn parse(
    input: &str,
    brackets: Option<(char, char)>,
    delimiter: Option<&str>,
) -> Result<Vec<Value>, String> {
    let result = parse_array(brackets, delimiter)(input)
        .map_err(|err| match err {
            nom::Err::Error(err) | nom::Err::Failure(err) => {
                // Create a descriptive error message if possible.
                nom::error::convert_error(input, err)
            }
            _ => err.to_string(),
        })
        .and_then(|(rest, result)| {
            rest.trim()
                .is_empty()
                .then(|| result)
                .ok_or_else(|| "could not parse successfully".into())
        })?;

    Ok(result)
}

fn parse_array<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    brackets: Option<(char, char)>,
    delimiter: Option<&'a str>,
) -> impl Fn(&'a str) -> IResult<&'a str, Vec<Value>, E> {
    let brackets = brackets.unwrap_or(('[', ']'));
    move |input| {
        preceded(
            char(brackets.0),
            terminated(cut(parse_array_values(delimiter)), char(brackets.1)),
        )(input)
    }
}

fn parse_array_values<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    delimiter: Option<&'a str>,
) -> impl Fn(&'a str) -> IResult<&'a str, Vec<Value>, E> {
    move |input| {
        let delimiter = delimiter.unwrap_or(",");
        // skip the last closing character
        separated_list0(tag(delimiter), cut(parse_value(delimiter)))(&input[..input.len() - 1])
            .and_then(|(rest, values)| {
                if rest.is_empty() {
                    // return the closing character
                    Ok((&input[input.len() - 1..], values))
                } else {
                    Ok((rest, values)) // will fail upstream
                }
            })
    }
}

fn parse_value<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    delimiter: &'a str,
) -> impl Fn(&'a str) -> IResult<&'a str, Value, E> {
    move |input| {
        map(
            alt((take_until(delimiter), take(input.len()))),
            |value: &str| Value::Bytes(Bytes::copy_from_slice(value.as_bytes())),
        )(input)
    }
}

pub fn filter_from_function(f: &Function) -> Result<GrokFilter, GrokStaticError> {
    let args_len = f.args.as_ref().map_or(0, |args| args.len());

    let mut delimiter = None;
    let mut value_filter = None;
    let mut brackets = None;
    if args_len == 1 {
        match &f.args.as_ref().unwrap()[0] {
            FunctionArgument::Arg(Value::Bytes(ref bytes)) => {
                delimiter = Some(String::from_utf8_lossy(bytes).to_string());
            }
            FunctionArgument::Function(f) => value_filter = Some(GrokFilter::try_from(f)?),
            _ => return Err(GrokStaticError::InvalidFunctionArguments(f.name.clone())),
        }
    } else if args_len == 2 {
        match (&f.args.as_ref().unwrap()[0], &f.args.as_ref().unwrap()[1]) {
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
        match (
            &f.args.as_ref().unwrap()[0],
            &f.args.as_ref().unwrap()[1],
            &f.args.as_ref().unwrap()[2],
        ) {
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
