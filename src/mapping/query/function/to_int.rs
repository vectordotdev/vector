use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct ToIntegerFn {
    query: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ToIntegerFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { query, default }
    }
}

impl Function for ToIntegerFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        match self.query.execute(ctx) {
            Ok(QueryValue::Value(value)) => {
                match value {
                    Value::Integer(_) => Ok(value.into()),
                    Value::Float(f) => Ok(Value::Integer(f as i64).into()),
                    Value::Bytes(_) => Conversion::Integer
                        .convert(value)
                        .map(Into::into)
                        .map_err(|e| e.to_string()),
                    Value::Boolean(b) => Ok(Value::Integer(if b { 1 } else { 0 }).into()),
                    Value::Timestamp(t) => Ok(Value::Integer(t.timestamp()).into()),
                    _ => unexpected_type!(value),
                }
            }
            Ok(query) => unexpected_type!(query),
            Err(err) => Err(err),
        }
        .or_else(|err| match &self.default {
            Some(v) => v.execute(ctx),
            None => Err(err),
        })
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: is_scalar_value,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: is_scalar_value,
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ToIntegerFn {
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
    fn to_int() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ToIntegerFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::Integer(10)),
                ToIntegerFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::Integer(10)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("20"));
                    event
                },
                Ok(Value::Integer(20)),
                ToIntegerFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Float(20.5));
                    event
                },
                Ok(Value::Integer(20)),
                ToIntegerFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
