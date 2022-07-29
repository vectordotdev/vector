use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn string(value: Value) -> Resolved {
    match value {
        v @ Value::Bytes(_) => Ok(v),
        v => Err(format!("expected string, got {}", v.kind()).into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct String;

impl Function for String {
    fn identifier(&self) -> &'static str {
        "string"
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
                source: r#"string("foobar")"#,
                result: Ok("foobar"),
            },
            Example {
                title: "invalid",
                source: "string!(true)",
                result: Err(
                    r#"function call error for "string" at (0:13): expected string, got boolean"#,
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

        Ok(Box::new(StringFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_string",
            address: vrl_fn_string as _,
            uses_context: false,
        })
    }
}

#[derive(Debug, Clone)]
struct StringFn {
    value: Box<dyn Expression>,
}

impl Expression for StringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        string(self.value.resolve(ctx)?)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        let non_bytes = !self.value.type_def(state).is_bytes();

        TypeDef::bytes().with_fallibility(non_bytes)
    }
}

#[no_mangle]
#[primitive_calling_convention]
pub extern "C" fn vrl_fn_string(value: Value) -> Resolved {
    string(value)
}
