use chrono::Utc;
use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Now;

impl Function for Now {
    fn identifier(&self) -> &'static str {
        "now"
    }

    fn compile(&self, _: ArgumentList) -> Result<Box<dyn Expression>> {
        Ok(Box::new(NowFn))
    }
}

#[derive(Debug, Clone)]
struct NowFn;

impl Expression for NowFn {
    fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
        Ok(Utc::now().into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: value::Kind::Timestamp,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    remap::test_type_def![static_def {
        expr: |_| NowFn,
        def: TypeDef {
            kind: value::Kind::Timestamp,
            ..Default::default()
        },
    }];
}
