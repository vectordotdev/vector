use crate::transforms::util::tokenize;
use remap::prelude::*;

#[derive(Debug)]
pub struct Tokenize;

impl Function for Tokenize {
    fn identifier(&self) -> &'static str {
        "tokenize"
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

        Ok(Box::new(TokenizeFn { value }))
    }
}

#[derive(Debug)]
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
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let value = {
            let bytes = required!(state, object, self.value, Value::String(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let tokens: Value = tokenize::parse(&value)
            .into_iter()
            .map(|token| match token {
                "" | "-" => Value::Null,
                _ => token.to_owned().into(),
            })
            .collect::<Vec<_>>()
            .into();

        Ok(Some(tokens))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn tokenize() {
        let cases = vec![(
                    map![],
                    Ok(Some(vec![
                            "217.250.207.207".into(),
                            Value::Null,
                            Value::Null,
                            "07/Sep/2020:16:38:00 -0400".into(),
                            "DELETE /deliverables/next-generation/user-centric HTTP/1.1".into(),
                            "205".into(),
                            "11881".into(),

                    ].into())),
                    TokenizeFn::new(Box::new(Literal::from("217.250.207.207 - - [07/Sep/2020:16:38:00 -0400] \"DELETE /deliverables/next-generation/user-centric HTTP/1.1\" 205 11881"))),
                )];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
