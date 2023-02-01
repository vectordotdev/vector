use ::value::Value;
use vrl::prelude::*;

fn split(value: Value, limit: Value, pattern: Value) -> Resolved {
    let string = value.try_bytes_utf8_lossy()?;
    let limit = limit.try_integer()? as usize;
    match pattern {
        Value::Regex(pattern) => Ok(pattern
            .splitn(string.as_ref(), limit)
            .collect::<Vec<_>>()
            .into()),
        Value::Bytes(bytes) => {
            let pattern = String::from_utf8_lossy(&bytes);

            Ok(string
                .splitn(limit, pattern.as_ref())
                .collect::<Vec<_>>()
                .into())
        }
        value => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::regex() | Kind::bytes(),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Split;

impl Function for Split {
    fn identifier(&self) -> &'static str {
        "split"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "pattern",
                kind: kind::BYTES | kind::REGEX,
                required: true,
            },
            Parameter {
                keyword: "limit",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "split string",
                source: r#"split("foobar", "b")"#,
                result: Ok(r#"["foo", "ar"]"#),
            },
            Example {
                title: "split once",
                source: r#"split("foobarbaz", "ba", 2)"#,
                result: Ok(r#"["foo", "rbaz"]"#),
            },
            Example {
                title: "split regex",
                source: r#"split("barbaz", r'ba')"#,
                result: Ok(r#"["", "r", "z"]"#),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");
        let limit = arguments.optional("limit").unwrap_or(expr!(999_999_999));

        Ok(SplitFn {
            value,
            pattern,
            limit,
        }
        .as_expr())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SplitFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
    limit: Box<dyn Expression>,
}

impl FunctionExpression for SplitFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let limit = self.limit.resolve(ctx)?;
        let pattern = self.pattern.resolve(ctx)?;

        split(value, limit, pattern)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::array(Collection::from_unknown(Kind::bytes())).infallible()
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod test {
    use super::*;

    test_function![
        split => Split;

        empty {
            args: func_args![value: "",
                             pattern: " "
            ],
            want: Ok(value!([""])),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }

        single {
            args: func_args![value: "foo",
                             pattern: " "
            ],
            want: Ok(value!(["foo"])),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }

        long {
            args: func_args![value: "This is a long string.",
                             pattern: " "
            ],
            want: Ok(value!(["This", "is", "a", "long", "string."])),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }

        regex {
            args: func_args![value: "This is a long string",
                             pattern: Value::Regex(regex::Regex::new(" ").unwrap().into()),
                             limit: 2
            ],
            want: Ok(value!(["This", "is a long string"])),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }

        non_space {
            args: func_args![value: "ThisaisAlongAstring.",
                             pattern: Value::Regex(regex::Regex::new("(?i)a").unwrap().into())
            ],
            want: Ok(value!(["This", "is", "long", "string."])),
            tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
        }

        unicode {
             args: func_args![value: "˙ƃuᴉɹʇs ƃuol ɐ sᴉ sᴉɥ┴",
                              pattern: " "
             ],
             want: Ok(value!(["˙ƃuᴉɹʇs", "ƃuol", "ɐ", "sᴉ", "sᴉɥ┴"])),
             tdef: TypeDef::array(Collection::from_unknown(Kind::bytes())),
         }

    ];
}
