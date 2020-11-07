use super::{
    CompilerState, Expr, Expression, Object, Result, State, TypeDef, Value, ValueConstraint,
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

            // TODO: make `Or` infallible, `Null`, `false` and `None` resolve to
            // rhs, everything else resolves to lhs
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

    fn type_def(&self, state: &CompilerState) -> TypeDef {
        use Operator::*;
        let constraint = match self.op {
            Or => self
                .lhs
                .type_def(state)
                .constraint
                .merge(&self.rhs.type_def(state).constraint),
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

        TypeDef {
            fallible: true,
            optional: false,
            constraint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_type_def, Literal, Noop, ValueConstraint::*, ValueKind::*};

    test_type_def![
        or_exact {
            expr: |_| Arithmetic::new(
                Box::new(Literal::from("foo").into()),
                Box::new(Literal::from(true).into()),
                Operator::Or,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: OneOf(vec![String, Boolean])
            },
        }

        or_any {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Literal::from(true).into()),
                Operator::Or,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: Any,
            },
        }

        multiply {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Multiply,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: OneOf(vec![String, Integer, Float]),
            },
        }

        add {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Add,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: OneOf(vec![String, Integer, Float]),
            },
        }

        remainder {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Remainder,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: OneOf(vec![Integer, Float]),
            },
        }

        subtract {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Subtract,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: OneOf(vec![Integer, Float]),
            },
        }

        divide {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Divide,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: OneOf(vec![Integer, Float]),
            },
        }

        and {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::And,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: Exact(Boolean),
            },
        }

        equal {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Equal,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: Exact(Boolean),
            },
        }

        not_equal {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::NotEqual,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: Exact(Boolean),
            },
        }

        greater {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Greater,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: Exact(Boolean),
            },
        }

        greater_or_equal {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::GreaterOrEqual,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: Exact(Boolean),
            },
        }

        less {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Less,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: Exact(Boolean),
            },
        }

        less_or_equal {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::LessOrEqual,
            ),
            def: TypeDef {
                fallible: true,
                optional: false,
                constraint: Exact(Boolean),
            },
        }
    ];
}
