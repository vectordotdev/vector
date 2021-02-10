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
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "substring",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "case_sensitive",
                kind: kind::ANY,
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
}

#[derive(Debug)]
struct EndsWithFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Option<Box<dyn Expression>>,
}

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

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let substring_def = self
            .substring
            .type_def(state)
            .fallible_unless(value::Kind::Bytes);

        let case_sensitive_def = self
            .case_sensitive
            .as_ref()
            .map(|cs| cs.type_def(state).fallible_unless(value::Kind::Boolean));

        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .merge(substring_def)
            .merge_optional(case_sensitive_def)
            .with_constraint(value::Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    
    vrl::test_type_def![
        value_string {
            expr: |_| EndsWithFn {
                value: Literal::from("foo").boxed(),
                substring: Literal::from("foo").boxed(),
                case_sensitive: None,
            },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        value_non_string {
            expr: |_| EndsWithFn {
                value: Literal::from(true).boxed(),
                substring: Literal::from("foo").boxed(),
                case_sensitive: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        substring_non_string {
            expr: |_| EndsWithFn {
                value: Literal::from("foo").boxed(),
                substring: Literal::from(true).boxed(),
                case_sensitive: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        case_sensitive_non_boolean {
            expr: |_| EndsWithFn {
                value: Literal::from("foo").boxed(),
                substring: Literal::from("foo").boxed(),
                case_sensitive: Some(Literal::from(1).boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }
    ];

    #[test]
    fn ends_with() {
        let cases = vec![
            (
                map![],
                Ok(false.into()),
                EndsWithFn::new(Box::new(Literal::from("bar")), "foo", false),
            ),
            (
                map![],
                Ok(false.into()),
                EndsWithFn::new(Box::new(Literal::from("bar")), "foobar", false),
            ),
            (
                map![],
                Ok(true.into()),
                EndsWithFn::new(Box::new(Literal::from("bar")), "bar", false),
            ),
            (
                map![],
                Ok(false.into()),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "oba", false),
            ),
            (
                map![],
                Ok(true.into()),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "bar", false),
            ),
            (
                map![],
                Ok(false.into()),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "foo", false),
            ),
            (
                map![],
                Ok(true.into()),
                EndsWithFn::new(Box::new(Literal::from("fooBAR")), "BAR", true),
            ),
            (
                map![],
                Ok(false.into()),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "BAR", true),
            ),
            (
                map![],
                Ok(true.into()),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "BAR", false),
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
