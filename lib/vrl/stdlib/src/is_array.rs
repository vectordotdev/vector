use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IsArray;

impl Function for IsArray {
    fn identifier(&self) -> &'static str {
        "is_array"
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
                title: "array",
                source: r#"is_array([1, 2, 3])"#,
                result: Ok("true"),
            },
            Example {
                title: "boolean",
                source: r#"is_array(true)"#,
                result: Ok("false"),
            },
            Example {
                title: "null",
                source: r#"is_array(null)"#,
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

        Ok(Box::new(IsArrayFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        Ok(value!(args.required("value").is_array()))
    }
}

#[derive(Clone, Debug)]
struct IsArrayFn {
    value: Box<dyn Expression>,
}

impl Expression for IsArrayFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        self.value.resolve(ctx).map(|v| value!(v.is_array()))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_array => IsArray;

        array {
            args: func_args![value: value!([1, 2, 3])],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        integer {
            args: func_args![value: value!(1789)],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
