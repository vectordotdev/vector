use vrl::prelude::*;

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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let substring = arguments.required("substring");
        let case_sensitive = arguments.optional("case_sensitive");

        Ok(Box::new(EndsWithFn {
            value,
            substring,
            case_sensitive,
        }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "ends with",
            source: r#"ends_with("the restaurant", "restaurant")"#,
            result: Ok("true"),
        }]
    }
}

#[derive(Clone, Debug)]
struct EndsWithFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Option<Box<dyn Expression>>,
}

/*
impl EndsWithFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, substring: &str, case_sensitive: bool) -> Self {
        let substring = Box::new(Literal::from(substring));
        let case_sensitive = Some(Box::new(Literal::from(case_sensitive)) as _);

        Self {
            value,
            substring,
            case_sensitive,
        }
    }
}
*/

impl Expression for EndsWithFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let case_sensitive = match &self.case_sensitive {
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
            None => false,
        };

        let substring = {
            let bytes = self.substring.resolve(ctx)?.try_bytes()?;
            let string = String::from_utf8_lossy(&bytes);

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

        Ok(value.ends_with(&substring).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
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
            tdef: TypeDef::new().infallible().boolean(),
        }

        opposite {
            args: func_args![value: "bar",
                             substring: "foobar"],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        subset {
            args: func_args![value: "foobar",
                             substring: "oba"],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        yes {
            args: func_args![value: "foobar",
                             substring: "bar"],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        starts_with {
            args: func_args![value: "foobar",
                             substring: "foo"],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        uppercase {
            args: func_args![value: "fooBAR",
                             substring: "BAR",
                             case_sensitive: true],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        case_sensitive {
            args: func_args![value: "foobar",
                             substring: "BAR",
                             case_sensitive: true],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        case_insensitive {
            args: func_args![value: "foobar",
                             substring: "BAR",
                             case_sensitive: false],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
