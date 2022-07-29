use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn is_integer(value: Value) -> Resolved {
    Ok(value.is_integer().into())
}

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
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IsIntegerFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_is_integer",
            address: vrl_fn_is_integer as _,
            uses_context: false,
        })
    }
}

#[derive(Clone, Debug)]
struct IsIntegerFn {
    value: Box<dyn Expression>,
}

impl Expression for IsIntegerFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        is_integer(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_is_integer(value: Value) -> Resolved {
    is_integer(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_integer => IsInteger;

        bytes {
            args: func_args![value: value!("foobar")],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        integer {
            args: func_args![value: value!(1789)],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
