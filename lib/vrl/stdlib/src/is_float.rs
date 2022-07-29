use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn is_float(value: Value) -> Resolved {
    Ok(value.is_float().into())
}

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
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IsFloatFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_is_float",
            address: vrl_fn_is_float as _,
            uses_context: false,
        })
    }
}

#[derive(Clone, Debug)]
struct IsFloatFn {
    value: Box<dyn Expression>,
}

impl Expression for IsFloatFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        is_float(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_is_float(value: Value) -> Resolved {
    is_float(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_float => IsFloat;

        bytes {
            args: func_args![value: value!("foobar")],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        float {
            args: func_args![value: value!(0.577)],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
