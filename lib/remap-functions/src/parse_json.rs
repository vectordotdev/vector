use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseJson;

impl Function for ParseJson {
    fn identifier(&self) -> &'static str {
        "parse_json"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ParseJsonFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ParseJsonFn {
    value: Box<dyn Expression>,
}

impl Expression for ParseJsonFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?;
        let bytes = value.try_bytes()?;
        let value = serde_json::from_slice::<'_, Value>(&bytes)
            .map_err(|e| format!("unable to parse json: {}", e))?;

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .into_fallible(true) // JSON parsing errors
            .with_constraint(
                Kind::Bytes
                    | Kind::Boolean
                    | Kind::Integer
                    | Kind::Float
                    | Kind::Array
                    | Kind::Map
                    | Kind::Null,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use value::Kind;

    test_function![
        parse_json => ParseJson;

        number {
            args: func_args![value: "42"],
            want: Ok(42),
        }

        string {
            args: func_args![value: r#""hello""#],
            want: Ok("hello"),
        }

        json {
            args: func_args![value: r#"{"field":"value"}"#],
            want: Ok(btreemap!{ "field" => "value" }),
        }

        invalid_value {
            args: func_args![value: r#"{ INVALID }"#],
            want: Err("function call error: unable to parse json: key must be a string at line 1 column 3"),
        }

        invalid_value_and_default {
            args: func_args![
                value: r#"{ INVALID }"#,
            ],
            want: Err("function call error: unable to parse json: key must be a string at line 1 column 3"),
        }
    ];

    test_type_def![value_string {
        expr: |_| ParseJsonFn {
            value: lit!("foo").boxed(),
        },
        def: TypeDef {
            fallible: true,
            kind: Kind::Bytes
                | Kind::Boolean
                | Kind::Integer
                | Kind::Float
                | Kind::Array
                | Kind::Map
                | Kind::Null,
            ..Default::default()
        },
    }];
}
