use super::prelude::*;
use crate::transforms::util::tokenize;

#[derive(Debug)]
pub(in crate::mapping) struct TokenizeFn {
    query: Box<dyn Function>,
}

impl TokenizeFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for TokenizeFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        self.query
            .execute(ctx)
            .map(|v| match v {
                Value::Bytes(v) => String::from_utf8_lossy(&v).into_owned(),
                v => unexpected_type!(v),
            })
            .map(|s| {
                tokenize::parse(&s)
                    .into_iter()
                    .map(|v| match v {
                        "" | "-" => Value::Null,
                        _ => Value::from(v.to_owned()),
                    })
                    .collect::<Vec<_>>()
            })
            .map(Value::from)
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for TokenizeFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize() {
        let cases = vec![(
                    Event::from(""),
                    Ok(Value::from(vec![
                            Value::from("217.250.207.207"),
                            Value::Null,
                            Value::Null,
                            Value::from("07/Sep/2020:16:38:00 -0400"),
                            Value::from("DELETE /deliverables/next-generation/user-centric HTTP/1.1"),
                            Value::from("205"),
                            Value::from("11881"),

                    ])),
                    TokenizeFn::new(Box::new(Literal::from(Value::from("217.250.207.207 - - [07/Sep/2020:16:38:00 -0400] \"DELETE /deliverables/next-generation/user-centric HTTP/1.1\" 205 11881")))),
                )];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn errors() {}
}
