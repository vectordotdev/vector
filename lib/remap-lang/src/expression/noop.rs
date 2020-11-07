use crate::{CompilerState, Expression, Object, Result, State, TypeCheck, Value};

#[derive(Debug, Clone)]
pub struct Noop;

impl Expression for Noop {
    fn execute(&self, _: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        Ok(None)
    }

    fn type_check(&self, _: &CompilerState) -> TypeCheck {
        TypeCheck {
            optional: true,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_type_check;

    test_type_check![noop {
        expr: |_| Noop,
        def: TypeCheck {
            optional: true,
            ..Default::default()
        },
    }];
}
