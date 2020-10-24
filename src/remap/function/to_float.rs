use crate::types::Conversion;
use remap::prelude::*;

#[derive(Debug)]
pub struct ToFloat;

impl Function for ToFloat {
    fn identifier(&self) -> &'static str {
        "to_float"
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

        Ok(Box::new(ToFloatFn { value, default }))
    }
}

#[derive(Debug)]
struct ToFloatFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToFloatFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { value, default }
    }
}

impl Expression for ToFloatFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        use Value::*;

        let to_float = |value| match value {
            Float(_) => Ok(value),
            Integer(v) => Ok(Float(v as f64)),
            Boolean(v) => Ok(Float(if v { 1.0 } else { 0.0 })),
            String(_) => Conversion::Float
                .convert(value.into())
                .map(Into::into)
                .map_err(|e| e.to_string().into()),
            _ => Err("unable to convert value to float".into()),
        };

        super::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_float,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use std::collections::BTreeMap;

    #[test]
    fn to_float() {
        let cases: Vec<(BTreeMap<String, Value>, _, _)> = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                ToFloatFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Ok(Some(Value::Float(10.0))),
                ToFloatFn::new(Box::new(Path::from("foo")), Some(Value::Float(10.0))),
            ),
            (
                map!["foo": "20.5"],
                Ok(Some(Value::Float(20.5))),
                ToFloatFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": 20],
                Ok(Some(Value::Float(20.0))),
                ToFloatFn::new(Box::new(Path::from("foo")), None),
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
