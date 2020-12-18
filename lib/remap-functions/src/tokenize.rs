use remap::prelude::*;
use shared::tokenize;

#[derive(Clone, Copy, Debug)]
pub struct Tokenize;

impl Function for Tokenize {
    fn identifier(&self) -> &'static str {
        "tokenize"
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

        Ok(Box::new(TokenizeFn { value }))
    }
}

#[derive(Debug, Clone)]
struct TokenizeFn {
    value: Box<dyn Expression>,
}

impl TokenizeFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for TokenizeFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let bytes = self.value.execute(state, object)?.try_bytes()?;
        let value = String::from_utf8_lossy(&bytes);

        let tokens: Value = tokenize::parse(&value)
            .into_iter()
            .map(|token| match token {
                "" | "-" => Value::Null,
                _ => token.to_owned().into(),
            })
            .collect::<Vec<_>>()
            .into();

        Ok(tokens)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Array)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| TokenizeFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: Kind::Array, ..Default::default() },
        }

        value_non_string {
            expr: |_| TokenizeFn { value: Literal::from(10).boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Array,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn tokenize() {
        let cases = vec![(
                    map![],
                    Ok(vec![
                            "217.250.207.207".into(),
                            Value::Null,
                            Value::Null,
                            "07/Sep/2020:16:38:00 -0400".into(),
                            "DELETE /deliverables/next-generation/user-centric HTTP/1.1".into(),
                            "205".into(),
                            "11881".into(),

                    ].into()),
                    TokenizeFn::new(Box::new(Literal::from("217.250.207.207 - - [07/Sep/2020:16:38:00 -0400] \"DELETE /deliverables/next-generation/user-centric HTTP/1.1\" 205 11881"))),
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
