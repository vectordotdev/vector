use super::Error as E;
use crate::{value, Expr, Expression, Object, Result, State, Value, ValueKind};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("invalid value kind")]
    Value(#[from] value::Error),
}

#[derive(Debug, Clone)]
pub(crate) struct IfStatement {
    conditional: Box<Expr>,
    true_expression: Box<Expr>,
    false_expression: Box<Expr>,
}

impl IfStatement {
    pub fn new(
        conditional: Box<Expr>,
        true_expression: Box<Expr>,
        false_expression: Box<Expr>,
    ) -> Self {
        Self {
            conditional,
            true_expression,
            false_expression,
        }
    }
}

impl Expression for IfStatement {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        match self.conditional.execute(state, object)? {
            Some(Value::Boolean(true)) => self.true_expression.execute(state, object),
            Some(Value::Boolean(false)) | None => self.false_expression.execute(state, object),
            Some(v) => Err(E::from(Error::from(value::Error::Expected(
                ValueKind::Boolean,
                v.kind(),
            )))
            .into()),
        }
    }
}
