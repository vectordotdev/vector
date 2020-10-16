use super::prelude::*;

#[derive(Debug)]
pub struct ToStringFn {
    query: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ToStringFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { query, default }
    }
}

impl Function for ToStringFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        match self.query.execute(ctx) {
            Ok(QueryValue::Value(value)) => match value {
                Value::Bytes(_) => Ok(value.into()),
                value => Ok(Value::Bytes(value.as_bytes()).into()),
            },
            Ok(query) => unexpected_type!(query),
            Err(err) => match &self.default {
                Some(v) => v.execute(ctx),
                None => Err(err),
            },
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |_| true,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |_| true,
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ToStringFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let default = arguments.optional("default");

        Ok(Self { query, default })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn to_string() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ToStringFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::from("default")),
                ToStringFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::from("default")),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(20));
                    event
                },
                Ok(Value::from("20")),
                ToStringFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Float(20.5));
                    event
                },
                Ok(Value::from("20.5")),
                ToStringFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
