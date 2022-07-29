use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn is_object(value: Value) -> Resolved {
    Ok(value.is_object().into())
}

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
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IsObjectFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_is_object",
            address: vrl_fn_is_object as _,
            uses_context: false,
        })
    }
}

#[derive(Clone, Debug)]
struct IsObjectFn {
    value: Box<dyn Expression>,
}

impl Expression for IsObjectFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        is_object(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::boolean().infallible()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_is_object(value: Value) -> Resolved {
    is_object(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_object => IsObject;

        bytes {
            args: func_args![value: value!("foobar")],
            want: Ok(value!(false)),
            tdef: TypeDef::boolean().infallible(),
        }

        object {
            args: func_args![value: value!({"foo": "bar"})],
            want: Ok(value!(true)),
            tdef: TypeDef::boolean().infallible(),
        }
    ];
}
