use shared::tokenize;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseTokens;

impl Function for ParseTokens {
    fn identifier(&self) -> &'static str {
        "parse_tokens"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "valid",
            // TODO: Remove `encode_json` hack.
            source: r#"encode_json(parse_tokens(s'A sentence "with \"a\" sentence inside" and [some brackets]'))"#,
            result: Ok(
                r##"s'["A","sentence","with \\\"a\\\" sentence inside","and","some brackets"]'"##,
            ),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(ParseTokensFn { value }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }
}

#[derive(Debug, Clone)]
struct ParseTokensFn {
    value: Box<dyn Expression>,
}

impl ParseTokensFn {
    /*
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
    */
}

impl Expression for ParseTokensFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.unwrap_bytes_utf8_lossy();

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

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().array_mapped::<(), Kind>(map! {
            (): Kind::Bytes
        })
    }
}

#[cfg(test)]
mod tests {
    /*
    use super::*;
    use crate::map;

    vrl::test_type_def![
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
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
    */
}
