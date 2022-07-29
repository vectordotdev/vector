use chrono::Utc;
use primitive_calling_convention::primitive_calling_convention;
use vrl::prelude::*;

fn now() -> Resolved {
    Ok(Utc::now().into())
}

#[derive(Clone, Copy, Debug)]
pub struct Now;

impl Function for Now {
    fn identifier(&self) -> &'static str {
        "now"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "now",
            source: r#"now() != """#,
            result: Ok("true"),
        }]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        _: ArgumentList,
    ) -> Compiled {
        Ok(Box::new(NowFn))
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_now",
            address: vrl_fn_now as _,
            uses_context: false,
        })
    }
}

#[derive(Debug, Clone)]
struct NowFn;

impl Expression for NowFn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        now()
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::timestamp()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_now() -> Resolved {
    now()
}
