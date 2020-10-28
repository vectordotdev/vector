use remap::prelude::*;

#[derive(Debug)]
pub struct ParseJson;

impl Function for ParseJson {
    fn identifier(&self) -> &'static str {
        "parse_json"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, Value::String(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let default = arguments.optional_expr("default")?;

        Ok(Box::new(ParseJsonFn { value, default }))
    }
}

#[derive(Debug)]
struct ParseJsonFn {
    value: Box<dyn Expression>,
    default: Option<Box<dyn Expression>>,
}

impl ParseJsonFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);

        Self { value, default }
    }
}

impl Expression for ParseJsonFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let to_json = |value| match value {
            Value::String(bytes) => serde_json::from_slice(&bytes)
                .map(|v: serde_json::Value| {
                    let v: crate::event::Value = v.into();
                    v.into()
                })
                .map_err(|err| format!("unable to parse json {}", err).into()),
            _ => Err(format!(r#"unable to convert value "{}" to json"#, value.kind()).into()),
        };

        super::convert_value_or_default(
            self.value.execute(state, object),
            self.default.as_ref().map(|v| v.execute(state, object)),
            to_json,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn parse_json() {
        let cases = vec![
            (
                map!["foo": "42"],
                Ok(Some(42.into())),
                ParseJsonFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": "\"hello\""],
                Ok(Some("hello".into())),
                ParseJsonFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": r#"{"field":"value"}"#],
                Ok(Some(map!["field": "value"].into())),
                ParseJsonFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": r#"{ INVALID }"#],
                Ok(Some(42.into())),
                ParseJsonFn::new(Box::new(Path::from("foo")), Some("42".into())),
            ),
            (
                map!["foo": r#"{ INVALID }"#],
                Err("function call error: unable to parse json key must be a string at line 1 column 3".into()),
                ParseJsonFn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map!["foo": r#"{ INVALID }"#],
                Err("function call error: unable to parse json key must be a string at line 1 column 3".into()),
                ParseJsonFn::new(Box::new(Path::from("foo")), Some("{ INVALID }".into())),
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
