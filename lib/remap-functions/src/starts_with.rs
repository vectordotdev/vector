use remap::prelude::*;

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
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "substring",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "case_sensitive",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let substring = arguments.required("substring")?.boxed();
        let case_sensitive = arguments.optional("case_sensitive").map(Expr::boxed);

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
    case_sensitive: Option<Box<dyn Expression>>,
}

impl StartsWithFn {
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

impl Expression for StartsWithFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let case_sensitive = match &self.case_sensitive {
            Some(expr) => expr.execute(state, object)?.try_boolean()?,
            None => false,
        };

        let substring = {
            let bytes = self.substring.execute(state, object)?.try_bytes()?;
            let string = String::from_utf8_lossy(&bytes);

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        let value = {
            let bytes = self.value.execute(state, object)?.try_bytes()?;
            let string = String::from_utf8_lossy(&bytes);

            match case_sensitive {
                true => string.into_owned(),
                false => string.to_lowercase(),
            }
        };

        Ok(value.starts_with(&substring).into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .merge(
                self.substring
                    .type_def(state)
                    .fallible_unless(value::Kind::Bytes),
            )
            .merge_optional(self.case_sensitive.as_ref().map(|case_sensitive| {
                case_sensitive
                    .type_def(state)
                    .fallible_unless(value::Kind::Boolean)
            }))
            .with_constraint(value::Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| StartsWithFn {
                value: Literal::from("foo").boxed(),
                substring: Literal::from("foo").boxed(),
                case_sensitive: None,
            },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        value_non_string {
            expr: |_| StartsWithFn {
                value: Literal::from(true).boxed(),
                substring: Literal::from("foo").boxed(),
                case_sensitive: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        substring_non_string {
            expr: |_| StartsWithFn {
                value: Literal::from("foo").boxed(),
                substring: Literal::from(true).boxed(),
                case_sensitive: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        case_sensitive_non_boolean {
            expr: |_| StartsWithFn {
                value: Literal::from("foo").boxed(),
                substring: Literal::from("foo").boxed(),
                case_sensitive: Some(Literal::from(1).boxed()),
            },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }
    ];

    #[test]
    fn starts_with() {
        let cases = vec![
            (
                btreemap! {},
                Ok(false.into()),
                StartsWithFn::new(Box::new(Literal::from("foo")), "bar", false),
            ),
            (
                btreemap! {},
                Ok(false.into()),
                StartsWithFn::new(Box::new(Literal::from("foo")), "foobar", false),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                StartsWithFn::new(Box::new(Literal::from("foo")), "foo", false),
            ),
            (
                btreemap! {},
                Ok(false.into()),
                StartsWithFn::new(Box::new(Literal::from("foobar")), "oba", false),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                StartsWithFn::new(Box::new(Literal::from("foobar")), "foo", false),
            ),
            (
                btreemap! {},
                Ok(false.into()),
                StartsWithFn::new(Box::new(Literal::from("foobar")), "bar", false),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                StartsWithFn::new(Box::new(Literal::from("FOObar")), "FOO", true),
            ),
            (
                btreemap! {},
                Ok(false.into()),
                StartsWithFn::new(Box::new(Literal::from("foobar")), "FOO", true),
            ),
            (
                btreemap! {},
                Ok(true.into()),
                StartsWithFn::new(Box::new(Literal::from("foobar")), "FOO", false),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
