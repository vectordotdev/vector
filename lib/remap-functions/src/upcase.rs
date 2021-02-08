use remap::prelude::*;
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug)]
pub struct Upcase;

impl Function for Upcase {
    fn identifier(&self) -> &'static str {
        "upcase"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(UpcaseFn { value }))
    }
}

#[derive(Debug, Clone)]
struct UpcaseFn {
    value: Box<dyn Expression>,
}

impl UpcaseFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for UpcaseFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        self.value
            .execute(state, object)
            .and_then(|v| String::try_from(v).map_err(Into::into))
            .map(|v| v.to_uppercase())
            .map(Into::into)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use value::Kind;

    remap::test_type_def![
        string {
            expr: |_| UpcaseFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        non_string {
            expr: |_| UpcaseFn { value: Literal::from(true).boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn upcase() {
        let cases = vec![(
            btreemap! { "foo" => "foo 2 bar" },
            Ok(Value::from("FOO 2 BAR")),
            UpcaseFn::new(Box::new(Path::from("foo"))),
        )];

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
