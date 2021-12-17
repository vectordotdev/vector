use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct EncodeJson;

impl Function for EncodeJson {
    fn identifier(&self) -> &'static str {
        "encode_json"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(EncodeJsonFn { value }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "encode object",
            source: r#"encode_json({"field": "value", "another": [1,2,3]})"#,
            result: Ok(r#"s'{"another":[1,2,3],"field":"value"}'"#),
        }]
    }
}

#[derive(Clone, Debug)]
struct EncodeJsonFn {
    value: Box<dyn Expression>,
}

impl Expression for EncodeJsonFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        // With `vrl::Value` it should not be possible to get `Err`.
        match serde_json::to_string(&value) {
            Ok(value) => Ok(value.into()),
            Err(error) => unreachable!("unable encode to json: {}", error),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value.type_def(state).infallible().bytes()
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use regex::Regex;

    use super::*;

    test_function![
        encode_json => EncodeJson;

        bytes {
            args: func_args![value: r#"hello"#],
            want: Ok(r#""hello""#),
            tdef: TypeDef::new().infallible().bytes(),
        }

        integer {
            args: func_args![value: 42],
            want: Ok("42"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        float {
            args: func_args![value: 42f64],
            want: Ok("42.0"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        boolean {
            args: func_args![value: false],
            want: Ok("false"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        map {
            args: func_args![value: map!["field": "value"]],
            want: Ok(r#"{"field":"value"}"#),
            tdef: TypeDef::new().infallible().bytes(),
        }

        array {
            args: func_args![value: vec![1, 2, 3]],
            want: Ok("[1,2,3]"),
            tdef: TypeDef::new().infallible().bytes(),
        }

        timestamp {
            args: func_args![
                value: DateTime::parse_from_str("1983 Apr 13 12:09:14.274 +0000", "%Y %b %d %H:%M:%S%.3f %z")
                    .unwrap()
                    .with_timezone(&Utc)
            ],
            want: Ok(r#""1983-04-13T12:09:14.274Z""#),
            tdef: TypeDef::new().infallible().bytes(),

        }

        regex {
            args: func_args![value: Regex::new("^a\\d+$").unwrap()],
            want: Ok(r#""^a\\d+$""#),
            tdef: TypeDef::new().infallible().bytes(),
        }

        null {
            args: func_args![value: Value::Null],
            want: Ok("null"),
            tdef: TypeDef::new().infallible().bytes(),
        }
    ];
}
