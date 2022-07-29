use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn is_boolean(value: Value) -> Resolved {
    Ok(value.is_boolean().into())
}

#[derive(Clone, Copy, Debug)]
pub struct IsBoolean;

impl Function for IsBoolean {
    fn identifier(&self) -> &'static str {
        "is_boolean"
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
                source: r#"is_boolean("foobar")"#,
                result: Ok("false"),
            },
            Example {
                title: "boolean",
                source: r#"is_boolean(false)"#,
                result: Ok("true"),
            },
            Example {
                title: "null",
                source: r#"is_boolean(null)"#,
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

        Ok(Box::new(IsBooleanFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_is_boolean",
            address: vrl_fn_is_boolean as _,
            uses_context: false,
        })
    }
}

#[derive(Clone, Debug)]
struct IsBooleanFn {
    value: Box<dyn Expression>,
}

impl Expression for IsBooleanFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        is_boolean(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_is_boolean(value: Value) -> Resolved {
    is_boolean(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_boolean => IsBoolean;

        bytes {
            args: func_args![value: value!("foobar")],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        array {
            args: func_args![value: value!(false)],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
