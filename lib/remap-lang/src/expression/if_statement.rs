use super::Error as E;
use crate::{
    value, CompilerState, Expr, Expression, Object, ValueConstraint, Result, State, Value, ValueKind,
};

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

    fn resolves_to(&self, state: &CompilerState) -> ValueConstraint {
        let true_resolves = self.true_expression.resolves_to(state);
        let false_resolves = self.false_expression.resolves_to(state);

        if true_resolves.is_any() || false_resolves.is_any() {
            return ValueConstraint::Any;
        }

        let true_value_kinds = true_resolves.value_kinds();
        let false_value_kinds = false_resolves.value_kinds();

        let kinds = true_value_kinds
            .into_iter()
            .chain(false_value_kinds.into_iter())
            .collect::<Vec<_>>();

        let resolve = if kinds.len() == 1 {
            ValueConstraint::Exact(kinds[0])
        } else {
            ValueConstraint::OneOf(kinds)
        };

        // FIXME: at this point, we might only have resolves from if or else, so
        // if the other is "maybe", the result can still be exact.
        if true_resolves.is_maybe() || false_resolves.is_maybe() {
            ValueConstraint::Maybe(Box::new(resolve))
        } else {
            resolve
        }
    }
}
