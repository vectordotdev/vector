use crate::types::Conversion;
use remap::prelude::*;

#[derive(Debug)]
pub struct ToInt;

impl Function for ToInt {
    fn identifier(&self) -> &'static str {
        "to_int"
    }

    fn parameters(&self) -> &'static [Parameter] {
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

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let default = arguments.optional_expr("default")?;

        Ok(Box::new(ToIntFn { value, default }))
    }
}

#[derive(Debug)]
struct ToIntFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToIntFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { query, default }
    }
}

impl Expression for ToIntFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        use Value::*;

        let to_int = |value| match value {
            Integer(_) => Ok(value),
            Float(v) => Ok(Integer(v as i64)),
            Boolean(v) => Ok(Integer(if v { 1 } else { 0 })),
            String(_) => Conversion::Integer
                .convert(value.into())
                .map(Into::into)
                .map_err(|e| e.to_string().into()),
            _ => Err("unable to convert value to integer".into()),
        };

        self.value
            .execute(state, object)
            .and_then(|opt| opt.map(to_int).transpose())
            .or_else(|err| {
                self.default
                    .as_ref()
                    .ok_or(err)
                    .and_then(|value| value.execute(state, object))
                    .and_then(|opt| opt.map(to_int).transpose())
            })
    }
}

fn is_scalar_value(value: &Value) -> bool {
    use Value::*;

    match value {
        Integer(_) | Float(_) | String(_) | Boolean(_) => true,
        Map(_) | Array(_) | Null => false,
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
