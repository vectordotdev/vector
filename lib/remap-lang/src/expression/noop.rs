use crate::{CompilerState, Expression, Object, Result, State, TypeDef, Value};

#[derive(Debug, Clone)]
pub struct Noop;

impl Expression for Noop {
    fn execute(&self, _: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        Ok(None)
    }

    fn type_def(&self, _: &CompilerState) -> TypeDef {
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
