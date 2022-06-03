use chrono::Utc;
use vrl::prelude::*;

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

    fn symbol(&self) -> Option<(&'static str, usize)> {
        // TODO
        None
    }
}

#[derive(Debug, Clone)]
struct NowFn;

impl Expression for NowFn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(Utc::now().into())
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::timestamp()
    }
}

#[inline(never)]
#[no_mangle]
pub extern "C" fn vrl_fn_now(value: &mut Value, result: &mut Resolved) {
    todo!()
}
