use ::value::Value;
use charset::Charset;
use data_encoding::BASE64_MIME;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_until1},
    combinator::{map, opt},
    error::{ContextError, ParseError},
    multi::fold_many1,
    sequence::{delimited, pair, separated_pair},
    IResult,
};
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct DecodeQ;

impl Function for DecodeQ {
    fn identifier(&self) -> &'static str {
        "decode_q"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(DecodeQFn { value }.as_expr())
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "Single",
                source: r#"decode_q!("=?utf-8?b?SGVsbG8sIFdvcmxkIQ==?=")"#,
                result: Ok(r#"Hello, World!"#),
            },
            Example {
                title: "Embedded",
                source: r#"decode_q!("From: =?utf-8?b?SGVsbG8sIFdvcmxkIQ==?= <=?utf-8?q?hello=5Fworld=40example=2ecom?=>")"#,
                result: Ok(r#"From: Hello, World! <hello_world@example.com>"#),
            },
            Example {
                title: "Without charset",
                source: r#"decode_q!("?b?SGVsbG8sIFdvcmxkIQ==")"#,
                result: Ok(r#"Hello, World!"#),
            },
        ]
    }
}

#[derive(Clone, Debug)]
struct DecodeQFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for DecodeQFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        decode_q(value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::bytes().fallible()
    }
}

fn decode_q(bytes: Value) -> Resolved {
    // Parse
    let input = bytes.try_bytes_utf8_lossy()?;
    let input: &str = &input;
    let (remaining, decoded) = alt((
        fold_many1(
            parse_delimited_q,
            || Ok(String::new()),
            |result, (head, word)| {
                let mut result = result?;

                result.push_str(head);
                if let Some(word) = word {
                    result.push_str(&word.decode_word()?);
                }

                Ok(result)
            },
        ),
        map(parse_internal_q, |word| word.decode_word()),
    ))(input)
    .map_err(|e| match e {
        nom::Err::Error(e) | nom::Err::Failure(e) => {
            // Create a descriptive error message if possible.
            nom::error::convert_error(input, e)
        }
        nom::Err::Incomplete(_) => e.to_string(),
    })?;
    let mut decoded = decoded?;

    // Add remaining input to the decoded string.
    decoded.push_str(remaining);

    Ok(decoded.into())
}

/// Parses input into (head, (charset, encoding, encoded text))
fn parse_delimited_q<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, (&'a str, Option<EncodedWord<'a>>), E> {
    pair(
        take_until("=?"),
        opt(delimited(tag("=?"), parse_internal_q, tag("?="))),
    )(input)
}

/// Parses inside of encoded word into (charset, encoding, encoded text)
fn parse_internal_q<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, EncodedWord<'a>, E> {
    map(
        separated_pair(
            opt(take_until1("?")),
            tag("?"),
            separated_pair(
                take_until("?"),
                tag("?"),
                alt((take_until("?="), |input| Ok(("", input)))),
            ),
        ),
        |(charset, (encoding, input))| EncodedWord {
            charset,
            encoding,
            input,
        },
    )(input)
}

struct EncodedWord<'a> {
    charset: Option<&'a str>,
    encoding: &'a str,
    input: &'a str,
}

impl<'a> EncodedWord<'a> {
    fn decode_word(&self) -> Result<String> {
        // Modified version from https://github.com/staktrace/mailparse/blob/a83d961fe53fd6504d75ee951a0e91dfea03c830/src/header.rs#L39

        // Decode
        let decoded = match self.encoding {
            "B" | "b" => BASE64_MIME
                .decode(self.input.as_bytes())
                .map_err(|_| "Unable to decode base64 value")?,
            "Q" | "q" => {
                // The quoted_printable module does a trim_end on the input, so if
                // that affects the output we should save and restore the trailing
                // whitespace
                let to_decode = self.input.replace('_', " ");
                let trimmed = to_decode.trim_end();
                let mut d = quoted_printable::decode(&trimmed, quoted_printable::ParseMode::Robust);
                if d.is_ok() && to_decode.len() != trimmed.len() {
                    d.as_mut()
                        .unwrap()
                        .extend_from_slice(to_decode[trimmed.len()..].as_bytes());
                }
                d.map_err(|_| "Unable to decode quoted_printable value")?
            }
            _ => return Err(format!("Invalid encoding: {:?}", self.encoding).into()),
        };

        // Convert to UTF-8
        let charset = self.charset.unwrap_or("utf-8");
        let charset = Charset::for_label_no_replacement(charset.as_bytes())
            .ok_or_else(|| format!("Unable to decode {:?} value", charset))?;
        let (cow, _) = charset.decode_without_bom_handling(&decoded);
        Ok(cow.into_owned())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use nom::error::VerboseError;

    #[test]
    fn internal() {
        let (remaining, word) =
            parse_internal_q::<VerboseError<&str>>("utf-8?Q?hello=5Fworld=40example=2ecom")
                .unwrap();
        assert_eq!(remaining, "");
        assert_eq!(word.charset, Some("utf-8"));
        assert_eq!(word.encoding, "Q");
        assert_eq!(word.input, "hello=5Fworld=40example=2ecom");
    }

    #[test]
    fn internal_no_charset() {
        let (remaining, word) =
            parse_internal_q::<VerboseError<&str>>("?Q?hello=5Fworld=40example=2ecom").unwrap();
        assert_eq!(remaining, "");
        assert_eq!(word.charset, None);
        assert_eq!(word.encoding, "Q");
        assert_eq!(word.input, "hello=5Fworld=40example=2ecom");
    }

    test_function![
        decode_q=> DecodeQ;

        non_utf8_charset {
            args: func_args![value: value!("Subject: =?iso-8859-1?Q?=A1Hola,_se=F1or!?=")],
            want: Ok(value!("Subject: ¡Hola, señor!")),
            tdef: TypeDef::bytes().fallible(),
        }

    ];
}
