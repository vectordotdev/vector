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

    fn compile(&self, _: ArgumentList) -> Compiled {
        Ok(Box::new(NowFn))
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
