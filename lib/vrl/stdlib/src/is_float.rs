use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IsFloat;

impl Function for IsFloat {
    fn identifier(&self) -> &'static str {
        "is_float"
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
                title: "float",
                source: r#"is_float(0.577)"#,
                result: Ok("true"),
            },
            Example {
                title: "boolean",
                source: r#"is_float(true)"#,
                result: Ok("false"),
            },
            Example {
                title: "null",
                source: r#"is_float(null)"#,
                result: Ok("false"),
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

        Ok(Box::new(IsFloatFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        Ok(value!(args.required("value").is_float()))
    }
}

#[derive(Clone, Debug)]
struct IsFloatFn {
    value: Box<dyn Expression>,
}

impl Expression for IsFloatFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.value.resolve(ctx).map(|v| value!(v.is_float()))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_float => IsFloat;

        bytes {
            args: func_args![value: value!("foobar")],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        float {
            args: func_args![value: value!(0.577)],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
