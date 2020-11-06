use crate::{CompilerState, Expression, Object, Result, State, TypeCheck, Value, ValueConstraint};

#[derive(Debug, Clone)]
pub struct Literal(Value);

impl<T: Into<Value>> From<T> for Literal {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl Expression for Literal {
    fn execute(&self, _: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        Ok(Some(self.0.clone()))
    }

    fn type_check(&self, _: &CompilerState) -> TypeCheck {
        TypeCheck {
            constraint: ValueConstraint::Exact(self.0.kind()),
            ..Default::default()
        }
    }
}
