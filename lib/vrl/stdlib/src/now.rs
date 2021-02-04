use chrono::Utc;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Now;

impl Function for Now {
    fn identifier(&self) -> &'static str {
        "now"
    }

    fn compile(&self, _: ArgumentList) -> Compiled {
        Ok(Box::new(NowFn))
    }
}

#[derive(Debug)]
struct NowFn;

impl Expression for NowFn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(Utc::now().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: Kind::Timestamp,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    vrl::test_type_def![static_def {
        expr: |_| NowFn,
        def: TypeDef {
            kind: value::Kind::Timestamp,
            ..Default::default()
        },
    }];
}
