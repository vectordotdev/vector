use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn is_array(value: Value) -> Resolved {
    Ok(value.is_array().into())
}

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
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IsArrayFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_is_array",
            address: vrl_fn_is_array as _,
            uses_context: false,
        })
    }
}

#[derive(Clone, Debug)]
struct IsArrayFn {
    value: Box<dyn Expression>,
}

impl Expression for IsArrayFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        is_array(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_is_array(value: Value) -> Resolved {
    is_array(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_array => IsArray;

        array {
            args: func_args![value: value!([1, 2, 3])],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }

        integer {
            args: func_args![value: value!(1789)],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
