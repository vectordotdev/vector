use vrl::prelude::*;

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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");
        let limit = arguments.optional("limit").unwrap_or(expr!(999999999));

        Ok(Box::new(SplitFn {
            value,
            pattern,
            limit,
        }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SplitFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
    limit: Box<dyn Expression>,
}

impl Expression for SplitFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let value = value.borrow();
        let string = value.try_bytes_utf8_lossy()?;

        let limit = self.limit.resolve(ctx)?;
        let limit = limit.borrow();
        let limit = limit.try_integer()? as usize;

        self.pattern.resolve(ctx).and_then(|pattern| {
            let pattern = pattern.borrow();

            match &*pattern {
                Value::Regex(pattern) => Ok(pattern
                    .splitn(string.as_ref(), limit as usize)
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
                    expected: Kind::Regex | Kind::Bytes,
                }
                .into()),
            }
        })
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new()
            .infallible()
            .array_mapped::<(), Kind>(map! {(): Kind::Bytes})
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
            tdef: TypeDef::new()
                .infallible()
                .array_mapped::<(), Kind>(map! {(): Kind::Bytes}),
        }

        single {
            args: func_args![value: "foo",
                             pattern: " "
            ],
            want: Ok(value!(["foo"])),
            tdef: TypeDef::new()
                .infallible()
                .array_mapped::<(), Kind>(map! {(): Kind::Bytes}),
        }

        long {
            args: func_args![value: "This is a long string.",
                             pattern: " "
            ],
            want: Ok(value!(["This", "is", "a", "long", "string."])),
            tdef: TypeDef::new()
                .infallible()
                .array_mapped::<(), Kind>(map! {(): Kind::Bytes}),
        }

        regex {
            args: func_args![value: "This is a long string",
                             pattern: Value::Regex(regex::Regex::new(" ").unwrap().into()),
                             limit: 2
            ],
            want: Ok(value!(["This", "is a long string"])),
            tdef: TypeDef::new()
                .infallible()
                .array_mapped::<(), Kind>(map! {(): Kind::Bytes}),
        }

        non_space {
            args: func_args![value: "ThisaisAlongAstring.",
                             pattern: Value::Regex(regex::Regex::new("(?i)a").unwrap().into())
            ],
            want: Ok(value!(["This", "is", "long", "string."])),
            tdef: TypeDef::new()
                .infallible()
                .array_mapped::<(), Kind>(map! {(): Kind::Bytes}),
        }

        unicode {
             args: func_args![value: "˙ƃuᴉɹʇs ƃuol ɐ sᴉ sᴉɥ┴",
                              pattern: " "
             ],
             want: Ok(value!(["˙ƃuᴉɹʇs", "ƃuol", "ɐ", "sᴉ", "sᴉɥ┴"])),
             tdef: TypeDef::new()
                .infallible()
                .array_mapped::<(), Kind>(map! {(): Kind::Bytes}),
         }

    ];
}
