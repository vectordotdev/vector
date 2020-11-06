use crate::{CompilerState, Expression, Object, ValueConstraint, Result, State, Value};

#[derive(Debug, Clone)]
pub struct Noop;

impl Expression for Noop {
    fn execute(&self, _: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        Ok(None)
    }

    fn resolves_to(&self, _: &CompilerState) -> ValueConstraint {
        ValueConstraint::Maybe(Box::new(ValueConstraint::Any))
    }
}
