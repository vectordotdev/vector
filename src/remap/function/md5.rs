use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Md5;

impl Function for Md5 {
    fn identifier(&self) -> &'static str {
        "md5"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::String(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;

        Ok(Box::new(Md5Fn { value }))
    }
}

#[derive(Debug, Clone)]
struct Md5Fn {
    value: Box<dyn Expression>,
}

impl Md5Fn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for Md5Fn {
    fn execute(
        &self,
        state: &mut state::Program,
        object: &mut dyn Object,
    ) -> Result<Option<Value>> {
        use md5::{Digest, Md5};

        self.value.execute(state, object).map(|r| {
            r.map(|v| match v.as_string_lossy() {
                Value::String(bytes) => Value::String(hex::encode(Md5::digest(&bytes)).into()),
                _ => unreachable!(),
            })
        })
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::String)
            .with_constraint(value::Kind::String)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| Md5Fn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: Kind::String, ..Default::default() },
        }

        value_non_string {
            expr: |_| Md5Fn { value: Literal::from(1).boxed() },
            def: TypeDef { fallible: true, kind: Kind::String, ..Default::default() },
        }

        value_optional {
            expr: |_| Md5Fn { value: Box::new(Noop) },
            def: TypeDef { fallible: true, optional: true, kind: Kind::String },
        }
    ];

    #[test]
    fn md5() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                Md5Fn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "foo"],
                Ok(Some(Value::from("acbd18db4cc2f85cedef654fccc4a4d8"))),
                Md5Fn::new(Box::new(Path::from("foo"))),
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
