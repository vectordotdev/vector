use super::{
    CompilerState, Expr, Expression, Object, Result, State, TypeCheck, Value, ValueConstraint,
    ValueKind,
};
use crate::Operator;

#[derive(Debug, Clone)]
pub struct Arithmetic {
    lhs: Box<Expr>,
    rhs: Box<Expr>,
    op: Operator,
}

impl Arithmetic {
    pub(crate) fn new(lhs: Box<Expr>, rhs: Box<Expr>, op: Operator) -> Self {
        Self { lhs, rhs, op }
    }
}

impl Expression for Arithmetic {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let lhs = self
            .lhs
            .execute(state, object)?
            .ok_or(super::Error::Missing)?;

        let rhs = self
            .rhs
            .execute(state, object)?
            .ok_or(super::Error::Missing)?;

        use Operator::*;
        let result = match self.op {
            Multiply => lhs.try_mul(rhs),
            Divide => lhs.try_div(rhs),
            Add => lhs.try_add(rhs),
            Subtract => lhs.try_sub(rhs),
            Or => lhs.try_or(rhs),
            And => lhs.try_and(rhs),
            Remainder => lhs.try_rem(rhs),
            Equal => Ok(lhs.eq_lossy(&rhs).into()),
            NotEqual => Ok((!lhs.eq_lossy(&rhs)).into()),
            Greater => lhs.try_gt(rhs),
            GreaterOrEqual => lhs.try_ge(rhs),
            Less => lhs.try_lt(rhs),
            LessOrEqual => lhs.try_le(rhs),
        };

        result.map(Some).map_err(Into::into)
    }

    fn type_check(&self, state: &CompilerState) -> TypeCheck {
        use Operator::*;
        let constraint = match self.op {
            Or => self
                .lhs
                .type_check(state)
                .constraint
                .merge(&self.rhs.type_check(state).constraint),
            Multiply | Add => ValueConstraint::OneOf(vec![
                ValueKind::String,
                ValueKind::Integer,
                ValueKind::Float,
            ]),
            Remainder | Subtract | Divide => {
                ValueConstraint::OneOf(vec![ValueKind::Integer, ValueKind::Float])
            }
            And | Equal | NotEqual | Greater | GreaterOrEqual | Less | LessOrEqual => {
                ValueConstraint::Exact(ValueKind::Boolean)
            }
        };

        TypeCheck {
            fallible: true,
            optional: false,
            constraint,
        }
    }
}
