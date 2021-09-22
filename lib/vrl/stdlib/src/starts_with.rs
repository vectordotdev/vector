use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct StartsWith;

impl Function for StartsWith {
    fn identifier(&self) -> &'static str {
        "starts_with"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "substring",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "case_sensitive",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "case sensitive",
                source: r#"starts_with("foobar", "F")"#,
                result: Ok("false"),
            },
            Example {
                title: "case insensitive",
                source: r#"starts_with("foobar", "F", false)"#,
                result: Ok("true"),
            },
            Example {
                title: "mismatch",
                source: r#"starts_with("foobar", "bar")"#,
                result: Ok("false"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _info: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let substring = arguments.required("substring");
        let case_sensitive = arguments.optional("case_sensitive").unwrap_or(expr!(true));

        Ok(Box::new(StartsWithFn {
            value,
            substring,
            case_sensitive,
        }))
    }
}

#[derive(Debug, Clone)]
struct StartsWithFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Box<dyn Expression>,
}

impl Expression for StartsWithFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let case_sensitive = self.case_sensitive.resolve(ctx)?.try_boolean()?;

        let substring = {
            let value = self.substring.resolve(ctx)?;
            let string = value.try_bytes_utf8_lossy()?;

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        let value = {
            let value = self.value.resolve(ctx)?;
            let string = value.try_bytes_utf8_lossy()?;

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        Ok(value.starts_with(&substring).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        starts_with => StartsWith;

        no {
            args: func_args![value: "foo",
                             substring: "bar"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }

        subset {
            args: func_args![value: "foo",
                             substring: "foobar"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }

        total {
            args: func_args![value: "foo",
                             substring: "foo"
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

        middle {
            args: func_args![value: "foobar",
                             substring: "oba"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }

        start {
            args: func_args![value: "foobar",
                             substring: "foo"
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

        end {
            args: func_args![value: "foobar",
                             substring: "bar"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }


        case_sensitive_same_case {
            args: func_args![value: "FOObar",
                             substring: "FOO"
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

        case_sensitive_different_case {
            args: func_args![value: "foobar",
                             substring: "FOO"
            ],
            want: Ok(false),
            tdef: TypeDef::new().infallible().boolean(),
        }

        case_insensitive_different_case {
            args: func_args![value: "foobar",
                             substring: "FOO",
                             case_sensitive: false
            ],
            want: Ok(true),
            tdef: TypeDef::new().infallible().boolean(),
        }

    ];
}
