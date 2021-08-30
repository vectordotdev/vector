use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_while, take_while1},
    character::complete::{char, satisfy},
    combinator::{consumed, cut, map, opt, recognize, value},
    error::{context, ContextError, ErrorKind, FromExternalError, ParseError},
    multi::{many1, separated_list0},
    number::complete::double,
    sequence::{preceded, separated_pair, terminated, tuple},
    AsChar, IResult, InputTakeAtPosition,
};
use std::num::ParseIntError;
use vrl::prelude::*;
use vrl::Value;

#[derive(Clone, Copy, Debug)]
pub struct ParseRubyHash;

impl Function for ParseRubyHash {
    fn identifier(&self) -> &'static str {
        "parse_ruby_hash"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "parse ruby hash",
            source: r#"parse_ruby_hash!(s'{ "test" => "value", "testNum" => 0.2, "testObj" => { "testBool" => true, "testNull" => nil } }')"#,
            result: Ok(r#"
                {
                    "test": "value",
                    "testNum": 0.2,
                    "testObj": {
                        "testBool": true,
                        "testNull": null
                    }
                }
            "#),
        }]
    }

    fn compile(&self, _state: &state::Compiler, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        Ok(Box::new(ParseRubyHashFn { value }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }
}

#[derive(Debug, Clone)]
struct ParseRubyHashFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseRubyHashFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let input = value.try_bytes_utf8_lossy()?;
        parse(&input)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        type_def()
    }
}

fn kinds() -> Kind {
    Kind::Null | Kind::Bytes | Kind::Float | Kind::Boolean | Kind::Array | Kind::Object
}

fn type_def() -> TypeDef {
    TypeDef::new()
        .fallible()
        .add_object::<(), Kind>(map! { (): kinds() })
}

trait HashParseError<T>: ParseError<T> + ContextError<T> + FromExternalError<T, ParseIntError> {}
impl<T, E: ParseError<T> + ContextError<T> + FromExternalError<T, ParseIntError>> HashParseError<T>
    for E
{
}

fn sp<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    let chars = " \t\r\n";

    take_while(move |c| chars.contains(c))(input)
}

fn parse_inner_str<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    delimiter: char,
) -> impl FnMut(&'a str) -> IResult<&'a str, &'a str, E> {
    move |input| {
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
        )(input)
    }
}

/// Parses text with a given delimiter.
fn parse_str<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    delimiter: char,
) -> impl FnMut(&'a str) -> IResult<&'a str, &'a str, E> {
    context(
        "string",
        preceded(
            char(delimiter),
            cut(terminated(parse_inner_str(delimiter), char(delimiter))),
        ),
    )
}

fn parse_boolean<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, bool, E> {
    let parse_true = value(true, tag("true"));
    let parse_false = value(false, tag("false"));

    alt((parse_true, parse_false))(input)
}

fn parse_nil<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Value, E> {
    value(Value::Null, tag("nil"))(input)
}

fn parse_bytes<'a, E: HashParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Bytes, E> {
    context(
        "bytes",
        map(alt((parse_str('"'), parse_str('\''))), |value| {
            Bytes::copy_from_slice(value.as_bytes())
        }),
    )(input)
}

fn parse_simple_key<T, E: ParseError<T>>(input: T) -> IResult<T, T, E>
where
    T: InputTakeAtPosition,
    <T as InputTakeAtPosition>::Item: AsChar,
{
    input.split_at_position1_complete(
        |item| {
            let c = item.as_char();
            !c.is_alphanum() && c.as_char() != '_'
        },
        ErrorKind::Complete,
    )
}

fn parse_colon_key<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    map(consumed(preceded(char(':'), parse_simple_key)), |res| res.0)(input)
}

fn parse_key<'a, E: HashParseError<&'a str>>(input: &'a str) -> IResult<&'a str, String, E> {
    context(
        "string",
        map(
            alt((
                parse_str('"'),
                parse_str('\''),
                parse_colon_key,
                parse_simple_key,
            )),
            String::from,
        ),
    )(input)
}

fn parse_array<'a, E: HashParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Value, E> {
    context(
        "array",
        map(
            preceded(
                char('['),
                cut(terminated(
                    separated_list0(preceded(sp, char(',')), parse_value),
                    preceded(sp, char(']')),
                )),
            ),
            Value::Array,
        ),
    )(input)
}

fn parse_key_value<'a, E: HashParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, (String, Value), E> {
    separated_pair(
        preceded(sp, parse_key),
        cut(preceded(sp, tag("=>"))),
        parse_value,
    )(input)
}

fn parse_hash<'a, E: HashParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Value, E> {
    context(
        "map",
        map(
            preceded(
                char('{'),
                cut(terminated(
                    map(
                        separated_list0(preceded(sp, char(',')), parse_key_value),
                        |tuple_vec| tuple_vec.into_iter().collect(),
                    ),
                    preceded(sp, char('}')),
                )),
            ),
            Value::Object,
        ),
    )(input)
}

fn parse_value<'a, E: HashParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Value, E> {
    preceded(
        sp,
        alt((
            parse_nil,
            parse_hash,
            parse_array,
            map(parse_bytes, Value::Bytes),
            map(double, |value| Value::Float(NotNan::new(value).unwrap())),
            map(parse_boolean, Value::Boolean),
        )),
    )(input)
}

fn parse(input: &str) -> Result<Value> {
    let result = parse_hash(input)
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
                .ok_or_else(|| "could not parse whole line successfully".into())
        })?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_object() {
        let result = parse("{}").unwrap();
        assert!(result.is_object());
    }

    #[test]
    fn test_parse_empty_array() {
        parse("{ array => [] }").unwrap();
    }

    #[test]
    fn test_parse_simple_object() {
        let result = parse(
            r#"{ "hello" => "world", "number" => 42, "float" => 4.2, "array" => [1, 2.3], "object" => { "nope" => nil } }"#,
        )
        .unwrap();
        assert!(result.is_object());
        let result = result.as_object().unwrap();
        assert!(result.get("hello").unwrap().is_bytes());
        assert!(result.get("number").unwrap().is_float());
        assert!(result.get("float").unwrap().is_float());
        assert!(result.get("array").unwrap().is_array());
        assert!(result.get("object").unwrap().is_object());
        let child = result.get("object").unwrap().as_object().unwrap();
        assert!(child.get("nope").unwrap().is_null());
    }

    #[test]
    fn test_parse_key_number() {
        let result = parse(r#"{ 42 => "hello world" }"#).unwrap();
        assert!(result.is_object());
        let result = result.as_object().unwrap();
        assert!(result.get("42").unwrap().is_bytes());
    }

    #[test]
    fn test_parse_key_colon() {
        let result = parse(r#"{ :colon => "hello world" }"#).unwrap();
        assert!(result.is_object());
        let result = result.as_object().unwrap();
        assert!(result.get(":colon").unwrap().is_bytes());
    }

    #[test]
    fn test_parse_underscore() {
        let result = parse(r#"{ with_underscore => "hello world" }"#).unwrap();
        assert!(result.is_object());
        let result = result.as_object().unwrap();
        assert!(result.get("with_underscore").unwrap().is_bytes());
    }

    #[test]
    fn test_parse_dash() {
        let result = parse(r#"{ "with-dash" => "foo" }"#).unwrap();
        assert!(result.is_object());
        let result = result.as_object().unwrap();
        assert!(result.get("with-dash").unwrap().is_bytes());
    }

    #[test]
    fn test_parse_quote() {
        let result = parse(r#"{ "with'quote" => "and\"double\"quote" }"#).unwrap();
        assert!(result.is_object());
        let result = result.as_object().unwrap();
        assert!(result.get("with'quote").unwrap().is_bytes());
    }

    #[test]
    fn test_parse_weird_format() {
        let result = parse(r#"{hello=>"world",'number'=>42,"weird"=>'format\'here'}"#).unwrap();
        assert!(result.is_object());
        let result = result.as_object().unwrap();
        assert!(result.get("hello").unwrap().is_bytes());
        assert!(result.get("number").unwrap().is_float());
    }

    #[test]
    fn test_non_hash() {
        assert!(parse(r#""hello world""#).is_err());
    }

    test_function![
        parse_ruby_hash => ParseRubyHash;

        complete {
            args: func_args![value: value!(r#"{ "test" => "value", "testNum" => 0.2, "testObj" => { "testBool" => true } }"#)],
            want: Ok(value!({
                test: "value",
                testNum: 0.2,
                testObj: {
                    testBool: true
                }
            })),
            tdef: type_def(),
        }
    ];
}
