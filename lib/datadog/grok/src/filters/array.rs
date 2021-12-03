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

use vrl_compiler::Value;

pub fn parse(
    input: &str,
    brackets: Option<(char, char)>,
    delimiter: Option<&str>,
) -> Result<Vec<Value>, String> {
    println!("{}", input);
    println!("{:?}", delimiter);
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
    fn parses_with_special_character_delimiter() {
        let result = parse("[1\t2]", None, Some("\t")).unwrap();
        assert_eq!(result, vec!["1".into(), "2".into()]);
    }

    #[test]
    fn parses_escaped_characters() {
        let result = parse("[1\\t2]", None, Some("\t")).unwrap();
        assert_eq!(result, vec!["1\\t2".into()]);
    }
}
