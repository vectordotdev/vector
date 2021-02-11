use vrl::prelude::*;

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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let substring = arguments.required("substring");
        let case_sensitive = arguments.optional("case_sensitive");

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
                source: r#"contains("banana", "ana")"#,
                result: Ok(r#"true"#),
            },
            Example {
                title: "case insensitive",
                source: r#"contains("banana", "AnA", case_sensitive: false)"#,
                result: Ok(r#"true"#),
            },
        ]
    }
}

#[derive(Clone, Debug)]
struct ContainsFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Option<Box<dyn Expression>>,
}

/*
impl ContainsFn {
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

impl Expression for ContainsFn {
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

        Ok(value.contains(&substring).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().boolean().infallible()
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    #[test]
    fn contains() {
        let cases = vec![
            (
                btreemap! {},
                Ok(false.into()),
                ContainsFn::new(Box::new(Literal::from("foo")), "bar", false),
            ),
            (
                btreemap! {},
                Ok(false.into()),
                ContainsFn::new(Box::new(Literal::from("foo")), "foobar", false),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                ContainsFn::new(Box::new(Literal::from("foo")), "foo", false),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                ContainsFn::new(Box::new(Literal::from("foobar")), "oba", false),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                ContainsFn::new(Box::new(Literal::from("foobar")), "foo", false),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                ContainsFn::new(Box::new(Literal::from("foobar")), "bar", false),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                ContainsFn::new(Box::new(Literal::from("fooBAR")), "BAR", true),
            ),
            (
                btreemap! {},
                Ok(false.into()),
                ContainsFn::new(Box::new(Literal::from("foobar")), "BAR", true),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                ContainsFn::new(Box::new(Literal::from("foobar")), "BAR", false),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
*/
