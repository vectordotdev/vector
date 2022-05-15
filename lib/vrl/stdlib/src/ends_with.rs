use ::value::Value;
use vrl::prelude::*;

enum Case {
    Sensitive,
    Insensitive,
}

fn ends_with(bytes: &Bytes, ends: &Bytes, case: Case) -> bool {
    if bytes.len() < ends.len() {
        return false;
    }

    match case {
        Case::Sensitive => ends[..] == bytes[bytes.len() - ends.len()..],
        Case::Insensitive => {
            return Chars::new(ends)
                .zip(Chars::new(&bytes.slice(bytes.len() - ends.len()..)))
                .all(|(a, b)| match (a, b) {
                    (Ok(a), Ok(b)) => {
                        if a.is_ascii() && b.is_ascii() {
                            a.to_ascii_lowercase() == b.to_ascii_lowercase()
                        } else {
                            a.to_lowercase().zip(b.to_lowercase()).all(|(a, b)| a == b)
                        }
                    }
                    _ => false,
                });
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EndsWith;

impl Function for EndsWith {
    fn identifier(&self) -> &'static str {
        "ends_with"
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

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let substring = arguments.required("substring");
        let case_sensitive = arguments.optional("case_sensitive").unwrap_or(expr!(true));

        Ok(Box::new(EndsWithFn {
            value,
            substring,
            case_sensitive,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "case sensitive",
                source: r#"ends_with("foobar", "R")"#,
                result: Ok("false"),
            },
            Example {
                title: "case insensitive",
                source: r#"ends_with("foobar", "R", false)"#,
                result: Ok("true"),
            },
            Example {
                title: "mismatch",
                source: r#"ends_with("foobar", "foo")"#,
                result: Ok("false"),
            },
        ]
    }

    fn call_by_vm(&self, _ctx: &Context, arguments: &mut VmArgumentList) -> Result<Value> {
        let value = arguments.required("value");
        let substring = arguments.required("substring");
        let case_sensitive = arguments
            .optional("case_sensitive")
            .map(|arg| arg.try_boolean())
            .transpose()?
            .unwrap_or(true);
        let substring = {
            let value = substring;
            let string = value.try_bytes_utf8_lossy()?;

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        let value = {
            let string = value.try_bytes_utf8_lossy()?;

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        Ok(value.ends_with(&substring).into())
    }
}

#[derive(Clone, Debug)]
struct EndsWithFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Box<dyn Expression>,
}

impl Expression for EndsWithFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx Context,
    ) -> Resolved<'value> {
        let case_sensitive = if self.case_sensitive.resolve(ctx)?.try_boolean()? {
            Case::Sensitive
        } else {
            Case::Insensitive
        };

        let substring = self.substring.resolve(ctx)?.try_bytes()?;
        let value = self.value.resolve(ctx)?.try_bytes()?;

        Ok(Cow::Owned(
            ends_with(&value, &substring, case_sensitive).into(),
        ))
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        ends_with => EndsWith;

        no {
            args: func_args![value: "bar",
                             substring: "foo"],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        opposite {
            args: func_args![value: "bar",
                             substring: "foobar"],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        subset {
            args: func_args![value: "foobar",
                             substring: "oba"],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        yes {
            args: func_args![value: "foobar",
                             substring: "bar"],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        starts_with {
            args: func_args![value: "foobar",
                             substring: "foo"],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        uppercase {
            args: func_args![value: "fooBAR",
                             substring: "BAR"
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        case_sensitive {
            args: func_args![value: "foobar",
                             substring: "BAR"
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        case_insensitive {
            args: func_args![value: "foobar",
                             substring: "BAR",
                             case_sensitive: false],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
