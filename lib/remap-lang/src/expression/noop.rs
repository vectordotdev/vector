use crate::{state, Expression, Object, Result, TypeDef, Value};

#[derive(Debug, Clone)]
pub struct Noop;

impl Expression for Noop {
    fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
        Ok(Value::Null)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            optional: true,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_type_def;

    test_type_def![noop {
        expr: |_| Noop,
        def: TypeDef {
            optional: true,
            ..Default::default()
        },
    }];
}
