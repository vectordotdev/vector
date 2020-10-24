use crate::types::Conversion;
use remap::prelude::*;

#[derive(Debug)]
pub struct ToBool;

impl Function for ToBool {
    fn identifier(&self) -> &'static str {
        "to_bool"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: super::is_scalar_value,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: super::is_scalar_value,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let default = arguments.optional_expr("default")?;

        Ok(Box::new(ToBoolFn { value, default }))
    }
}

#[derive(Debug)]
struct ToBoolFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToBoolFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { value, default }
    }
}

impl Expression for ToBoolFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        use Value::*;

        let to_bool = |value| match value {
            Boolean(_) => Ok(value),
            Integer(v) => Ok(Boolean(v != 0)),
            Float(v) => Ok(Boolean(v != 0.0)),
            String(_) => Conversion::Boolean
                .convert(value.into())
                .map(Into::into)
                .map_err(|e| e.to_string().into()),
            _ => Err("unable to convert value to boolean".into()),
        };

        super::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_bool,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn to_bool() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                ToBoolFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Ok(Some(Value::Boolean(true))),
                ToBoolFn::new(Box::new(Path::from("foo")), Some(Value::Boolean(true))),
            ),
            (
                map!["foo": "true"],
                Ok(Some(Value::Boolean(true))),
                ToBoolFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": 20],
                Ok(Some(Value::Boolean(true))),
                ToBoolFn::new(Box::new(Path::from("foo")), None),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
