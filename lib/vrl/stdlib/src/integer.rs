use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn int(value: Value) -> Resolved {
    match value {
        v @ Value::Integer(_) => Ok(v),
        v => Err(format!(r#"expected integer, got {}"#, v.kind()).into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Integer;

impl Function for Integer {
    fn identifier(&self) -> &'static str {
        "int"
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
                title: "valid",
                source: r#"int(42)"#,
                result: Ok("42"),
            },
            Example {
                title: "invalid",
                source: "int!(true)",
                result: Err(
                    r#"function call error for "int" at (0:10): expected integer, got boolean"#,
                ),
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

        Ok(Box::new(IntegerFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_int",
            address: vrl_fn_int as _,
            uses_context: false,
        })
    }
}

#[derive(Debug, Clone)]
struct IntegerFn {
    value: Box<dyn Expression>,
}

impl Expression for IntegerFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        int(self.value.resolve(ctx)?)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        let non_integer = !self.value.type_def(state).is_integer();

        TypeDef::integer().with_fallibility(non_integer)
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_int(value: Value) -> Resolved {
    int(value)
}
