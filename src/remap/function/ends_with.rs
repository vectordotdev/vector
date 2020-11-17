use remap::prelude::*;

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
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "substring",
                accepts: |v| matches!(v, Value::String(_)),
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
        let value = arguments.required_expr("value")?;
        let substring = arguments.required_expr("substring")?;
        let case_sensitive = arguments.optional_expr("case_sensitive")?;

        Ok(Box::new(EndsWithFn {
            value,
            substring,
            case_sensitive,
        }))
    }
}

#[derive(Debug, Clone)]
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
    fn execute(
        &self,
        state: &mut state::Program,
        object: &mut dyn Object,
    ) -> Result<Option<Value>> {
        let substring = {
            let bytes = required!(state, object, self.substring, Value::String(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let value = {
            let bytes = required!(state, object, self.value, Value::String(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let ends_with = value.ends_with(&substring)
            || optional!(state, object, self.case_sensitive, Value::Boolean(b) => b)
                .iter()
                .filter(|&case_sensitive| !case_sensitive)
                .any(|_| value.to_lowercase().ends_with(&substring.to_lowercase()));

        Ok(Some(ends_with.into()))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let substring_def = self
            .substring
            .type_def(state)
            .fallible_unless(value::Kind::String);

        let case_sensitive_def = self
            .case_sensitive
            .as_ref()
            .map(|cs| cs.type_def(state).fallible_unless(value::Kind::Boolean));

        self.value
            .type_def(state)
            .fallible_unless(value::Kind::String)
            .merge(substring_def)
            .merge_optional(case_sensitive_def)
            .with_constraint(value::Kind::Boolean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use value::Kind;

    remap::test_type_def![
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
                Err("path error: missing path: foo".into()),
                EndsWithFn::new(Box::new(Path::from("foo")), "", false),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("bar")), "foo", false),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("bar")), "foobar", false),
            ),
            (
                map![],
                Ok(Some(true.into())),
                EndsWithFn::new(Box::new(Literal::from("bar")), "bar", false),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "oba", false),
            ),
            (
                map![],
                Ok(Some(true.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "bar", false),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "foo", false),
            ),
            (
                map![],
                Ok(Some(true.into())),
                EndsWithFn::new(Box::new(Literal::from("fooBAR")), "BAR", true),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "BAR", true),
            ),
            (
                map![],
                Ok(Some(true.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "BAR", false),
            ),
        ];

        let mut state = state::Program::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
