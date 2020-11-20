use super::{Expr, Expression, Object, Result, TypeDef, Value};
use crate::{state, value, Operator};

#[derive(Debug, Clone)]
pub struct Arithmetic {
    lhs: Box<Expr>,
    rhs: Box<Expr>,
    op: Operator,
}

impl Arithmetic {
    pub fn new(lhs: Box<Expr>, rhs: Box<Expr>, op: Operator) -> Self {
        Self { lhs, rhs, op }
    }
}

impl Expression for Arithmetic {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        use Operator::*;

        let lhs = self.lhs.execute(state, object)?;
        let rhs = self.rhs.execute(state, object)?;

        match self.op {
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
        }
        .map_err(Into::into)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;
        use Operator::*;

        let kind = match self.op {
            Or => self.lhs.type_def(state).kind | self.rhs.type_def(state).kind,
            Multiply | Add => Kind::Bytes | Kind::Integer | Kind::Float,
            Remainder | Subtract | Divide => Kind::Integer | Kind::Float,
            And | Equal | NotEqual | Greater | GreaterOrEqual | Less | LessOrEqual => Kind::Boolean,
        };

        TypeDef {
            fallible: true,
            optional: false,
            kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expression::{Literal, Noop},
        test_type_def,
        value::Kind,
    };

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
                kind: Kind::Bytes | Kind::Boolean,
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
                kind: Kind::all(),
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
                kind: Kind::Bytes | Kind::Integer | Kind::Float,
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
                kind: Kind::Bytes | Kind::Integer | Kind::Float,
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
                kind: Kind::Integer | Kind::Float,
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
                kind: Kind::Integer | Kind::Float,
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
                kind: Kind::Integer | Kind::Float,
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
                kind: Kind::Boolean,
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
                kind: Kind::Boolean,
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
                kind: Kind::Boolean,
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
                kind: Kind::Boolean,
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
                kind: Kind::Boolean,
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
                kind: Kind::Boolean,
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
                kind: Kind::Boolean,
            },
        }
    ];
}
