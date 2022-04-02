use vrl::prelude::*;

fn replace(value: Value, with_value: Value, count: Value, pattern: Value) -> Resolved {
    let value = value.try_bytes_utf8_lossy()?;
    let with = with_value.try_bytes_utf8_lossy()?;
    let count = count.try_integer()?;
    match pattern {
        Value::Bytes(bytes) => {
            let pattern = String::from_utf8_lossy(&bytes);
            let replaced = match count {
                i if i > 0 => value.replacen(pattern.as_ref(), &with, i as usize),
                i if i < 0 => value.replace(pattern.as_ref(), &with),
                _ => value.into_owned(),
            };

            Ok(replaced.into())
        }
        Value::Regex(regex) => {
            let replaced = match count {
                i if i > 0 => Bytes::copy_from_slice(
                    regex.replacen(&value, i as usize, with.as_ref()).as_bytes(),
                )
                .into(),
                i if i < 0 => {
                    Bytes::copy_from_slice(regex.replace_all(&value, with.as_ref()).as_bytes())
                        .into()
                }
                _ => value.into(),
            };

            Ok(replaced)
        }
        value => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::regex() | Kind::bytes(),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Replace;

impl Function for Replace {
    fn identifier(&self) -> &'static str {
        "replace"
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
                keyword: "with",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "count",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "replace all",
                source: r#"replace("foobar", "o", "i")"#,
                result: Ok("fiibar"),
            },
            Example {
                title: "replace count",
                source: r#"replace("foobar", "o", "i", count: 1)"#,
                result: Ok("fiobar"),
            },
            Example {
                title: "replace regex",
                source: r#"replace("foobar", r'o|a', "i")"#,
                result: Ok("fiibir"),
            },
            Example {
                title: "replace with capture group",
                source: r#"replace("foo123bar", r'foo(?P<num>\d+)bar', "$num")"#,
                result: Ok(r#""123""#),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");
        let with = arguments.required("with");
        let count = arguments.optional("count").unwrap_or(expr!(-1));

        Ok(Box::new(ReplaceFn {
            value,
            pattern,
            with,
            count,
        }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        let pattern = args.required("pattern");
        let with = args.required("with");
        let count = args.optional("count").unwrap_or_else(|| value!(-1));

        replace(value, with, count, pattern)
    }
}

#[derive(Debug, Clone)]
struct ReplaceFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
    with: Box<dyn Expression>,
    count: Box<dyn Expression>,
}

impl Expression for ReplaceFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let with_value = self.with.resolve(ctx)?;
        let count = self.count.resolve(ctx)?;
        let pattern = self.pattern.resolve(ctx)?;

        replace(value, with_value, count, pattern)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::bytes().infallible()
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod test {
    use super::*;

    test_function![
        replace => Replace;

        replace_string1 {
             args: func_args![value: "I like apples and bananas",
                              pattern: "a",
                              with: "o"
             ],
             want: Ok("I like opples ond bononos"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_string2 {
             args: func_args![value: "I like apples and bananas",
                              pattern: "a",
                              with: "o",
                              count: -1
             ],
             want: Ok("I like opples ond bononos"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_string3 {
             args: func_args![value: "I like apples and bananas",
                              pattern: "a",
                              with: "o",
                              count: 0
             ],
             want: Ok("I like apples and bananas"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_string4 {
             args: func_args![value: "I like apples and bananas",
                              pattern: "a",
                              with: "o",
                              count: 1
             ],
             want: Ok("I like opples and bananas"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_string5 {
             args: func_args![value: "I like apples and bananas",
                              pattern: "a",
                              with: "o",
                              count: 2
             ],
             want: Ok("I like opples ond bananas"),
             tdef: TypeDef::bytes().infallible(),
         }


        replace_regex1 {
             args: func_args![value: "I like opples ond bananas",
                              pattern: regex::Regex::new("a").unwrap(),
                              with: "o"
             ],
             want: Ok("I like opples ond bononos"),
             tdef: TypeDef::bytes().infallible(),
         }


        replace_regex2 {
             args: func_args![value: "I like apples and bananas",
                              pattern: regex::Regex::new("a").unwrap(),
                              with: "o",
                              count: -1
             ],
             want: Ok("I like opples ond bononos"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_regex3 {
             args: func_args![value: "I like apples and bananas",
                              pattern: regex::Regex::new("a").unwrap(),
                              with: "o",
                              count: 0
             ],
             want: Ok("I like apples and bananas"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_regex4 {
             args: func_args![value: "I like apples and bananas",
                              pattern: regex::Regex::new("a").unwrap(),
                              with: "o",
                              count: 1
             ],
             want: Ok("I like opples and bananas"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_regex5 {
             args: func_args![value: "I like apples and bananas",
                              pattern: regex::Regex::new("a").unwrap(),
                              with: "o",
                              count: 2
             ],
             want: Ok("I like opples ond bananas"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_other {
            args: func_args![value: "I like apples and bananas",
                             pattern: "apples",
                             with: "biscuits"
            ],
             want: Ok( "I like biscuits and bananas"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_other2 {
             args: func_args![value: "I like apples and bananas",
                              pattern: regex::Regex::new("a").unwrap(),
                              with: "o",
                              count: 1
             ],
             want: Ok("I like opples and bananas"),
             tdef: TypeDef::bytes().infallible(),
         }

        replace_other3 {
            args: func_args![value: "I like [apples] and bananas",
                             pattern: regex::Regex::new("\\[apples\\]").unwrap(),
                             with: "biscuits"
            ],
            want: Ok("I like biscuits and bananas"),
            tdef: TypeDef::bytes().infallible(),
        }
    ];
}
