use crate::{state, value, Expression, Object, Result, TypeDef, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Noop;

impl Expression for Noop {
    fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
        Ok(Value::Null)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: value::Kind::Null,
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
            kind: value::Kind::Null,
            ..Default::default()
        },
    }];
}
