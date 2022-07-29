use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn upcase(value: Value) -> Resolved {
    Ok(value.try_bytes_utf8_lossy()?.to_uppercase().into())
}

#[derive(Clone, Copy, Debug)]
pub struct Upcase;

impl Function for Upcase {
    fn identifier(&self) -> &'static str {
        "upcase"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "upcase",
            source: r#"upcase("foo 2 bar")"#,
            result: Ok("FOO 2 BAR"),
        }]
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::BYTES,
            required: true,
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(UpcaseFn { value }))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_upcase",
            address: vrl_fn_upcase as _,
            uses_context: false,
        })
    }
}

#[derive(Debug, Clone)]
struct UpcaseFn {
    value: Box<dyn Expression>,
}

impl Expression for UpcaseFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        upcase(value)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::bytes().infallible()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_upcase(value: Value) -> Resolved {
    upcase(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        upcase => Upcase;

        simple {
            args: func_args![value: "FOO 2 bar"],
            want: Ok(value!("FOO 2 BAR")),
            tdef: TypeDef::bytes(),
        }
    ];
}
