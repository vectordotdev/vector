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
struct ToStringFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ToStringFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
        Self { value, default }
    }
}

impl Expression for ToStringFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let to_string = |value| match value {
            Value::String(_) => Ok(value),
            _ => Ok(value.as_string_lossy()),
        };

        super::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_string,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use std::collections::BTreeMap;

    #[test]
    fn to_string() {
        let cases: Vec<(BTreeMap<String, Value>, _, _)> = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                ToStringFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Ok(Some(Value::from("default"))),
                ToStringFn::new(Box::new(Path::from("foo")), Some(Value::from("default"))),
            ),
            (
                map!["foo": 20],
                Ok(Some(Value::from("20"))),
                ToStringFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": 20.5],
                Ok(Some(Value::from("20.5"))),
                ToStringFn::new(Box::new(Path::from("foo")), None),
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
