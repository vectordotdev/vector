use regex::Regex;
use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Match;

impl Function for Match {
    fn identifier(&self) -> &'static str {
        "match"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |_| true,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let pattern = arguments.required_regex("pattern")?;

        Ok(Box::new(MatchFn { value, pattern }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MatchFn {
    value: Box<dyn Expression>,
    pattern: Regex,
}

impl MatchFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, pattern: Regex) -> Self {
        Self { value, pattern }
    }
}

impl Expression for MatchFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?;
        let string = value.try_bytes_utf8_lossy()?;

        Ok(self.pattern.is_match(&string).into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Boolean)
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod tests {
    use super::*;
    use shared::btreemap;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| MatchFn {
                value: Literal::from("foo").boxed(),
                pattern: Regex::new("").unwrap(),
            },
            def: TypeDef { kind: Kind::Boolean, ..Default::default() },
        }

        value_non_string {
            expr: |_| MatchFn {
                value: Literal::from(1).boxed(),
                pattern: Regex::new("").unwrap(),
            },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }

        value_optional {
            expr: |_| MatchFn {
                value: Box::new(Noop),
                pattern: Regex::new("").unwrap(),
            },
            def: TypeDef { fallible: true, kind: Kind::Boolean, ..Default::default() },
        }
    ];

    #[test]
    fn r#match() {
        let cases = vec![
            (
                btreemap! { "foo" => "foobar" },
                Ok(false.into()),
                MatchFn::new(Box::new(Path::from("foo")), Regex::new("\\s\\w+").unwrap()),
            ),
            (
                btreemap! { "foo" => "foo 2 bar" },
                Ok(true.into()),
                MatchFn::new(
                    Box::new(Path::from("foo")),
                    Regex::new("foo \\d bar").unwrap(),
                ),
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
