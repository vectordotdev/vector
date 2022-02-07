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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        _: ArgumentList,
    ) -> Compiled {
        Ok(Box::new(NowFn))
    }

    fn call_by_vm(&self, _ctx: &mut Context, _args: &mut VmArgumentList) -> Resolved {
        Ok(Utc::now().into())
    }
}

#[derive(Debug, Clone)]
struct NowFn;

impl Expression for NowFn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(Utc::now().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().timestamp()
    }
}
