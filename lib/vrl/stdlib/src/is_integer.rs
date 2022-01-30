use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IsInteger;

impl Function for IsInteger {
    fn identifier(&self) -> &'static str {
        "is_integer"
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
                source: r#"is_integer("foobar")"#,
                result: Ok("false"),
            },
            Example {
                title: "integer",
                source: r#"is_integer(1515)"#,
                result: Ok("true"),
            },
            Example {
                title: "null",
                source: r#"is_integer(null)"#,
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

        Ok(Box::new(IsIntegerFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        Ok(value!(args.required("value").is_integer()))
    }
}

#[derive(Clone, Debug)]
struct IsIntegerFn {
    value: Box<dyn Expression>,
}

impl Expression for IsIntegerFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.value.resolve(ctx).map(|v| value!(v.is_integer()))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_integer => IsInteger;

        bytes {
            args: func_args![value: value!("foobar")],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        integer {
            args: func_args![value: value!(1789)],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
