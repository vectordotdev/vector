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
            And if lhs_def.kind.is_null() => lhs_def.with_constraint(Kind::Boolean),
            And => type_def
                .fallible_unless(Kind::Null | Kind::Boolean)
                .with_constraint(Kind::Boolean),
            Equal | NotEqual => type_def.with_constraint(Kind::Boolean),
            Greater | GreaterOrEqual | Less | LessOrEqual => type_def
                .fallible_unless(Kind::Integer | Kind::Float)
                .with_constraint(Kind::Boolean),
            Subtract | Divide | Remainder => type_def
                .fallible_unless(Kind::Integer | Kind::Float)
                .with_constraint(Kind::Integer | Kind::Float),
            IntegerDivide => type_def
                .fallible_unless(Kind::Integer | Kind::Float)
                .with_constraint(Kind::Integer),
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
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::Divide,
            ),
            def: TypeDef {
                fallible: true,
                kind: Kind::Integer | Kind::Float,
                ..Default::default()
            },
        }

        integer_divide {
            expr: |_| Arithmetic::new(
                Box::new(Noop.into()),
                Box::new(Noop.into()),
                Operator::IntegerDivide,
            ),
            def: TypeDef {
                fallible: true,
                kind: Kind::Integer
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
    ];
}
