use remap::prelude::*;
use shared::tokenize;

#[derive(Clone, Copy, Debug)]
pub struct ParseTokens;

impl Function for ParseTokens {
    fn identifier(&self) -> &'static str {
        "parse_tokens"
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

        Ok(Box::new(ParseTokensFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ParseTokensFn {
    value: Box<dyn Expression>,
}

impl ParseTokensFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for ParseTokensFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?;
        let string = value.try_bytes_utf8_lossy()?;

        let tokens: Value = tokenize::parse(&string)
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
    use shared::btreemap;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| ParseTokensFn { value: Literal::from("foo").boxed() },
            def: TypeDef { kind: Kind::Array, ..Default::default() },
        }

        value_non_string {
            expr: |_| ParseTokensFn { value: Literal::from(10).boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Array,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn parse_tokens() {
        let cases = vec![(
                    btreemap!{},
                    Ok(vec![
                            "217.250.207.207".into(),
                            Value::Null,
                            Value::Null,
                            "07/Sep/2020:16:38:00 -0400".into(),
                            "DELETE /deliverables/next-generation/user-centric HTTP/1.1".into(),
                            "205".into(),
                            "11881".into(),

                    ].into()),
                    ParseTokensFn::new(Box::new(Literal::from("217.250.207.207 - - [07/Sep/2020:16:38:00 -0400] \"DELETE /deliverables/next-generation/user-centric HTTP/1.1\" 205 11881"))),
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
