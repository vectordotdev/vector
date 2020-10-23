use remap::prelude::*;

#[derive(Debug)]
pub struct ToString;

impl Function for ToString {
    fn identifier(&self) -> &'static str {
        "to_string"
    }

    fn parameters(&self) -> &'static [Parameter] {
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

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let default = arguments.optional_expr("default")?;

        Ok(Box::new(ToStringFn { value, default }))
    }
}

#[derive(Debug)]
pub struct ToStringFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToStringFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { query, default }
    }
}

impl Expression for ToStringFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        self.value
            .execute(state, object)
            .map(|value| match value {
                Some(v @ Value::String(_)) => Some(v),
                Some(value) => Some(value.as_string_lossy()),
                None => Some(Value::String("".into())),
            })
            .or_else(|err| match &self.default {
                Some(default) => default.execute(state, object),
                None => Err(err),
            })
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
