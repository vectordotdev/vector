use ::value::Value;
use vrl::prelude::*;

fn is_json(value: Value) -> Resolved {
    let bytes = value.try_bytes()?;

    match serde_json::from_slice::<'_, serde::de::IgnoredAny>(&bytes) {
        Ok(_) => Ok(value!(true)),
        Err(_) => Ok(value!(false)),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IsJson;

impl Function for IsJson {
    fn identifier(&self) -> &'static str {
        "is_json"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "object",
                source: r#"is_json("{}")"#,
                result: Ok("true"),
            },
            Example {
                title: "string",
                source: r#"is_json(s'"test"')"#,
                result: Ok("true"),
            },
            Example {
                title: "invalid",
                source: r#"is_json("}{")"#,
                result: Ok("false"),
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        Ok(Box::new(IsJsonFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        is_json(args.required("value"))
    }
}

#[derive(Clone, Debug)]
struct IsJsonFn {
    value: Box<dyn Expression>,
}

impl Expression for IsJsonFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        is_json(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_json => IsJson;

        object {
            args: func_args![value: r#"{}"#],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        string {
            args: func_args![value: r#""test""#],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        invalid {
            args: func_args![value: r#"}{"#],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

    ];
}
