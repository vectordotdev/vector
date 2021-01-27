use super::{Expr, Expression, Object, Result, TypeDef, Value};
use crate::{state, value, Operator};

#[derive(Debug, Clone, PartialEq)]
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

        if matches!(self.op, ErrorOr) {
            return self
                .lhs
                .execute(state, object)
                .or_else(|_| self.rhs.execute(state, object));
        }

        let lhs = self.lhs.execute(state, object)?;
        let rhs = self.rhs.execute(state, object)?;

        match self.op {
            Multiply => lhs.try_mul(rhs),
            Divide => lhs.try_div(rhs),
            IntegerDivide => lhs.try_int_div(rhs),
            Add => lhs.try_add(rhs),
            Subtract => lhs.try_sub(rhs),
            Or => Ok(lhs.or(rhs)),
            And => lhs.try_and(rhs),
            Remainder => lhs.try_rem(rhs),
            Equal => Ok(lhs.eq_lossy(&rhs).into()),
            NotEqual => Ok((!lhs.eq_lossy(&rhs)).into()),
            Greater => lhs.try_gt(rhs),
            GreaterOrEqual => lhs.try_ge(rhs),
            Less => lhs.try_lt(rhs),
            LessOrEqual => lhs.try_le(rhs),
            ErrorOr => unreachable!(),
        }
        .map_err(Into::into)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;
        use Operator::*;

        let lhs_def = self.lhs.type_def(state);
        let rhs_def = self.rhs.type_def(state);
        let type_def = lhs_def.clone() | rhs_def.clone();

        match self.op {
            Or if lhs_def.kind.is_null() => rhs_def,
            Or if !lhs_def.kind.is_boolean() => lhs_def,
            Or => type_def,
            ErrorOr if !lhs_def.is_fallible() => lhs_def,
            ErrorOr if !rhs_def.is_fallible() => rhs_def,
            ErrorOr => type_def,
            And if lhs_def.kind.is_null() => lhs_def.with_constraint(Kind::Boolean),
            And => type_def
                .fallible_unless(Kind::Null | Kind::Boolean)
                .with_constraint(Kind::Boolean),
            Equal | NotEqual => type_def.with_constraint(Kind::Boolean),
            Greater | GreaterOrEqual | Less | LessOrEqual => type_def
                .fallible_unless(Kind::Integer | Kind::Float)
                .with_constraint(Kind::Boolean),
            Subtract | Remainder => type_def
                .fallible_unless(Kind::Integer | Kind::Float)
                .with_constraint(Kind::Integer | Kind::Float),
            Divide => type_def.into_fallible(true).with_constraint(Kind::Float),
            IntegerDivide => type_def.into_fallible(true).with_constraint(Kind::Integer),
            Multiply | Add => type_def
                .fallible_unless(Kind::Bytes | Kind::Integer | Kind::Float)
                .with_constraint(Kind::Bytes | Kind::Integer | Kind::Float),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expression::{Literal, Noop},
        lit, test_type_def,
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
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        or_null {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Literal::from(true).into()),
                Operator::Or,
            ),
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
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
                kind: Kind::Bytes | Kind::Integer | Kind::Float,
                ..Default::default()
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
                kind: Kind::Bytes | Kind::Integer | Kind::Float,
                ..Default::default()
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
                kind: Kind::Integer | Kind::Float,
                ..Default::default()
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
                kind: Kind::Integer | Kind::Float,
                ..Default::default()
            },
        }

        divide {
            expr: |_| Arithmetic::new(
                Box::new(10.into()),
                Box::new(5.into()),
                Operator::Divide,
            ),
            def: TypeDef {
                fallible: true,
                kind: Kind::Float,
                ..Default::default()
            },
        }

        integer_divide {
            expr: |_| Arithmetic::new(
                Box::new(8.into()),
                Box::new(4.into()),
                Operator::IntegerDivide,
            ),
            def: TypeDef {
                fallible: true,
                kind: Kind::Integer,
                ..Default::default()
            },
        }

        and {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::And,
            ),
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        equal {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Equal,
            ),
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        not_equal {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::NotEqual,
            ),
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
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
                kind: Kind::Boolean,
                ..Default::default()
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
                kind: Kind::Boolean,
                ..Default::default()
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
                kind: Kind::Boolean,
                ..Default::default()
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
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        error_or_lhs_infallible {
            expr: |_| Arithmetic::new(
                Box::new(Expr::from(lit!("foo"))),
                Box::new(Arithmetic::new(
                    Box::new(Expr::from(lit!("foo"))),
                    Box::new(Expr::from(lit!(1))),
                    Operator::Divide,
                ).into()),
                Operator::ErrorOr,
            ),
            def: TypeDef {
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        error_or_rhs_infallible {
            expr: |_| Arithmetic::new(
                Box::new(Arithmetic::new(
                    Box::new(Expr::from(lit!("foo"))),
                    Box::new(Expr::from(lit!(1))),
                    Operator::Divide,
                ).into()),
                Box::new(Expr::from(lit!(true))),
                Operator::ErrorOr,
            ),
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        error_or_fallible {
            expr: |_| Arithmetic::new(
                Box::new(Arithmetic::new(
                    Box::new(Expr::from(lit!("foo"))),
                    Box::new(Expr::from(lit!(1))),
                    Operator::Divide,
                ).into()),
                Box::new(Arithmetic::new(
                    Box::new(Expr::from(lit!(true))),
                    Box::new(Expr::from(lit!(1))),
                    Operator::Divide,
                ).into()),
                Operator::ErrorOr,
            ),
            def: TypeDef {
                kind: Kind::Float,
                fallible: true,
                ..Default::default()
            },
        }

        error_or_nested_infallible {
            expr: |_| Arithmetic::new(
                Box::new(Arithmetic::new(
                    Box::new(Expr::from(lit!("foo"))),
                    Box::new(Expr::from(lit!(1))),
                    Operator::Divide,
                ).into()),
                Box::new(Arithmetic::new(
                    Box::new(Arithmetic::new(
                        Box::new(Expr::from(lit!(true))),
                        Box::new(Expr::from(lit!(1))),
                        Operator::Divide,
                    ).into()),
                    Box::new(Expr::from(lit!("foo"))),
                    Operator::ErrorOr,
                ).into()),
                Operator::ErrorOr,
            ),
            def: TypeDef {
                kind: Kind::Bytes,
                ..Default::default()
            },
        }
    ];
}
