use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IsObject;

impl Function for IsObject {
    fn identifier(&self) -> &'static str {
        "is_object"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "string",
                source: r#"is_object("foobar")"#,
                result: Ok("false"),
            },
            Example {
                title: "boolean",
                source: r#"is_object(true)"#,
                result: Ok("false"),
            },
            Example {
                title: "object",
                source: r#"is_object({"foo": "bar"})"#,
                result: Ok("true"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IsObjectFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        Ok(value!(args.required("value").is_object()))
    }
}

#[derive(Clone, Debug)]
struct IsObjectFn {
    value: Box<dyn Expression>,
}

impl Expression for IsObjectFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.value.resolve(ctx).map(|v| value!(v.is_object()))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_object => IsObject;

        bytes {
            args: func_args![value: value!("foobar")],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        object {
            args: func_args![value: value!({"foo": "bar"})],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
