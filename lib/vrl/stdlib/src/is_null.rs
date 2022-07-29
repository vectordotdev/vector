use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn is_null(value: Value) -> Resolved {
    Ok(value.is_null().into())
}

#[derive(Clone, Copy, Debug)]
pub struct IsNull;

impl Function for IsNull {
    fn identifier(&self) -> &'static str {
        "is_null"
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
                source: r#"is_null([1, 2, 3])"#,
                result: Ok("false"),
            },
            Example {
                title: "string",
                source: r#"is_null("foobar")"#,
                result: Ok("false"),
            },
            Example {
                title: "null",
                source: r#"is_null(null)"#,
                result: Ok("true"),
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

        Ok(Box::new(IsNullFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_is_null",
            address: vrl_fn_is_null as _,
            uses_context: false,
        })
    }
}

#[derive(Clone, Debug)]
struct IsNullFn {
    value: Box<dyn Expression>,
}

impl Expression for IsNullFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        is_null(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_is_null(value: Value) -> Resolved {
    is_null(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_null => IsNull;

        array {
            args: func_args![value: value!(null)],
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
