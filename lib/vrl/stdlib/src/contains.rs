use vrl::prelude::*;

fn contains(value: Value, substring: Value, case_sensitive: bool) -> Resolved {
    let substring = {
        let bytes = substring.try_bytes()?;
        let string = String::from_utf8_lossy(&bytes);

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
    Ok(value.contains(&substring).into())
}

#[derive(Clone, Copy, Debug)]
pub struct Contains;

impl Function for Contains {
    fn identifier(&self) -> &'static str {
        "contains"
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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let substring = arguments.required("substring");
        let case_sensitive = arguments.optional("case_sensitive").unwrap_or(expr!(true));

        Ok(Box::new(ContainsFn {
            value,
            substring,
            case_sensitive,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "case sensitive",
                source: r#"contains("banana", "AnA")"#,
                result: Ok(r#"false"#),
            },
            Example {
                title: "case insensitive",
                source: r#"contains("banana", "AnA", case_sensitive: false)"#,
                result: Ok(r#"true"#),
            },
        ]
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        let substring = args.required("substring");
        let case_sensitive = args
            .optional("case_sensitive")
            .map(|value| value.try_boolean().unwrap_or(true))
            .unwrap_or(true);

        contains(value, substring, case_sensitive)
    }
}

#[derive(Clone, Debug)]
struct ContainsFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Box<dyn Expression>,
}

impl Expression for ContainsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let substring = self.substring.resolve(ctx)?;
        let case_sensitive = self.case_sensitive.resolve(ctx)?.try_boolean()?;

        contains(value, substring, case_sensitive)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().boolean().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        contains => Contains;

        no {
            args: func_args![value: value!("foo"),
                             substring: value!("bar")],
            want: Ok(value!(false)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        yes {
            args: func_args![value: value!("foobar"),
                             substring: value!("foo")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        entirely {
            args: func_args![value: value!("foo"),
                             substring: value!("foo")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        middle {
            args: func_args![value: value!("foobar"),
                             substring: value!("oba")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        start {
            args: func_args![value: value!("foobar"),
                             substring: value!("foo")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        end {
            args: func_args![value: value!("foobar"),
                             substring: value!("bar")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        case_sensitive_yes {
            args: func_args![value: value!("fooBAR"),
                             substring: value!("BAR"),
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }

         case_sensitive_yes_lowercase {
            args: func_args![value: value!("fooBAR"),
                             substring: value!("bar"),
                             case_sensitive: true
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        case_sensitive_no_uppercase {
            args: func_args![value: value!("foobar"),
                             substring: value!("BAR"),
            ],
            want: Ok(value!(false)),
            tdef: TypeDef::new().boolean().infallible(),
        }

        case_insensitive_yes_uppercase {
            args: func_args![value: value!("foobar"),
                             substring: value!("BAR"),
                             case_sensitive: false
            ],
            want: Ok(value!(true)),
            tdef: TypeDef::new().boolean().infallible(),
        }
    ];
}
