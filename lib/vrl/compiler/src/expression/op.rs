use crate::expression::{Expr, Noop, Resolved};
use crate::parser::{ast, Node};
use crate::{value, Context, Expression, State, TypeDef, Value};
use diagnostic::{DiagnosticError, Label, Span};
use std::fmt;

#[derive(Clone, PartialEq)]
pub struct Op {
    pub(crate) lhs: Box<Expr>,
    pub(crate) rhs: Box<Expr>,
    pub(crate) opcode: ast::Opcode,
}

impl Op {
    pub fn new(lhs: Expr, opcode: Node<ast::Opcode>, rhs: Expr) -> Result<Self, Error> {
        use ast::Opcode::*;

        let (span, opcode) = opcode.take();

        if matches!(opcode, Eq | Ne | Lt | Le | Gt | Ge) {
            if let Expr::Op(op) = &lhs {
                if matches!(op.opcode, Eq | Ne | Lt | Le | Gt | Ge) {
                    let error = Error::ChainedComparison { span };
                    return std::result::Result::Err(error);
                }
            }
        }

        Ok(Op {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            opcode,
        })
    }

    pub fn noop() -> Self {
        let lhs = Box::new(Noop.into());
        let rhs = Box::new(Noop.into());

        Op {
            lhs,
            rhs,
            opcode: ast::Opcode::Eq,
        }
    }
}

impl Expression for Op {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use ast::Opcode::*;
        use Value::*;

        let lhs = self.lhs.resolve(ctx);
        let mut rhs = || self.rhs.resolve(ctx);

        match self.opcode {
            Mul => lhs?.try_mul(rhs()?),
            Div => lhs?.try_div(rhs()?),
            Add => lhs?.try_add(rhs()?),
            Sub => lhs?.try_sub(rhs()?),
            Rem => lhs?.try_rem(rhs()?),
            Or => lhs?.try_or(rhs),
            And => match lhs? {
                Null | Boolean(false) => Ok(false.into()),
                v => v.try_and(rhs()?),
            },
            Err => Ok(lhs.or_else(|_| rhs())?),
            Eq => Ok(lhs?.eq_lossy(&rhs()?).into()),
            Ne => Ok((!lhs?.eq_lossy(&rhs()?)).into()),
            Gt => lhs?.try_gt(rhs()?),
            Ge => lhs?.try_ge(rhs()?),
            Lt => lhs?.try_lt(rhs()?),
            Le => lhs?.try_le(rhs()?),
        }
        .map_err(Into::into)
    }

    fn type_def(&self, state: &State) -> TypeDef {
        use ast::Opcode::*;
        use value::Kind as K;

        let lhs_def = self.lhs.type_def(state);
        let rhs_def = self.rhs.type_def(state);
        let merged_def = lhs_def.clone().merge(rhs_def.clone());

        let lhs_kind = lhs_def.kind();
        let rhs_kind = rhs_def.kind();
        let merged_kind = merged_def.kind();

        match self.opcode {
            // null || null
            Or if merged_kind.is_null() => TypeDef::new().infallible().null(),

            // null || ...
            Or if lhs_kind.is_null() => rhs_def,

            // "foo" || ...
            Or if !lhs_kind.is_boolean() => lhs_def,

            // ... || ...
            Or => merged_def,

            // ok ?? ...
            Err if lhs_def.is_infallible() => lhs_def,

            // ok/err ?? ok
            Err if rhs_def.is_infallible() => merged_def.infallible(),

            // ... ?? ...
            Err => merged_def,

            // null && ...
            And if lhs_kind.is_null() => rhs_def.scalar(K::Boolean),

            // ... && ...
            And => merged_def
                .fallible_unless(K::Null | K::Boolean)
                .scalar(K::Boolean),

            // ... == ...
            // ... != ...
            Eq | Ne => merged_def.boolean(),

            // ... >  ...
            // ... >= ...
            // ... <  ...
            // ... <= ...
            Gt | Ge | Lt | Le => merged_def
                .fallible_unless(K::Integer | K::Float)
                .scalar(K::Boolean),

            // ... / ...
            Div => merged_def.fallible().float(),

            // "bar" + ...
            // ... + "bar"
            Add if lhs_kind.is_bytes() || rhs_kind.is_bytes() => merged_def
                .fallible_unless(K::Bytes | K::Null)
                .scalar(K::Bytes),

            // ... + 1.0
            // ... - 1.0
            // ... * 1.0
            // ... % 1.0
            // 1.0 + ...
            // 1.0 - ...
            // 1.0 * ...
            // 1.0 % ...
            Add | Sub | Mul | Rem if lhs_kind.is_float() || rhs_kind.is_float() => merged_def
                .fallible_unless(K::Integer | K::Float)
                .scalar(K::Float),

            // 1 + 1
            // 1 - 1
            // 1 * 1
            // 1 % 1
            Add | Sub | Mul | Rem if lhs_kind.is_integer() && rhs_kind.is_integer() => {
                merged_def.infallible().scalar(K::Integer)
            }

            // "bar" * 1
            Mul if lhs_kind.is_bytes() && rhs_kind.is_integer() => {
                merged_def.infallible().scalar(K::Bytes)
            }

            // 1 * "bar"
            Mul if lhs_kind.is_integer() && rhs_kind.is_bytes() => {
                merged_def.infallible().scalar(K::Bytes)
            }

            // ... + ...
            // ... * ...
            Add | Mul => merged_def
                .fallible()
                .scalar(K::Bytes | K::Integer | K::Float),

            // ... - ...
            // ... % ...
            Sub | Rem => merged_def.fallible().scalar(K::Integer | K::Float),
        }
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.lhs, self.opcode, self.rhs)
    }
}

impl fmt::Debug for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Op({} {} {})", self.lhs, self.opcode, self.rhs)
    }
}

// -----------------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("comparison operators cannot be chained")]
    ChainedComparison { span: Span },
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use Error::*;

        match self {
            ChainedComparison { .. } => 650,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::*;

        match self {
            ChainedComparison { span } => vec![Label::primary("", span)],
        }
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::Literal;
    use crate::{test_type_def, value::Kind};
    use ast::Opcode::*;
    use ordered_float::NotNan;

    fn op(opcode: ast::Opcode, lhs: impl Into<Literal>, rhs: impl Into<Literal>) -> Op {
        Op {
            lhs: Box::new(lhs.into().into()),
            rhs: Box::new(rhs.into().into()),
            opcode,
        }
    }

    fn f(f: f64) -> NotNan<f64> {
        NotNan::new(f).unwrap()
    }

    test_type_def![
        or_exact {
            expr: |_| op(Or, "foo", true),
            want: TypeDef::new().bytes(),
        }

        or_null {
            expr: |_| op(Or, (), true),
            want: TypeDef::new().boolean(),
        }

        multiply_string_integer {
            expr: |_| op(Mul, "foo", 1),
            want: TypeDef::new().bytes(),
        }

        multiply_integer_string {
            expr: |_| op(Mul, 1, "foo"),
            want: TypeDef::new().bytes(),
        }

        multiply_float_integer {
            expr: |_| op(Mul, f(1.0), 1),
            want: TypeDef::new().float(),
        }

        multiply_integer_float {
            expr: |_| op(Mul, 1, f(1.0)),
            want: TypeDef::new().float(),
        }

        multiply_integer_integer {
            expr: |_| op(Mul, 1, 1),
            want: TypeDef::new().integer(),
        }

        multiply_other {
            expr: |_| op(Mul, (), ()),
            want: TypeDef::new().fallible().scalar(Kind::Bytes | Kind::Integer | Kind::Float),
        }

        add_string_string {
            expr: |_| op(Add, "foo", "bar"),
            want: TypeDef::new().bytes(),
        }

        add_string_null {
            expr: |_| op(Add, "foo", ()),
            want: TypeDef::new().bytes(),
        }

        add_null_string {
            expr: |_| op(Add, (), "foo"),
            want: TypeDef::new().bytes(),
        }

        add_string_bool {
            expr: |_| op(Add, "foo", true),
            want: TypeDef::new().fallible().bytes(),
        }

        add_float_integer {
            expr: |_| op(Add, f(1.0), 1),
            want: TypeDef::new().float(),
        }

        add_integer_float {
            expr: |_| op(Add, 1, f(1.0)),
            want: TypeDef::new().float(),
        }

        add_float_other {
            expr: |_| op(Add, f(1.0), ()),
            want: TypeDef::new().fallible().float(),
        }

        add_other_float {
            expr: |_| op(Add, (), f(1.0)),
            want: TypeDef::new().fallible().float(),
        }

        add_integer_integer {
            expr: |_| op(Add, 1, 1),
            want: TypeDef::new().integer(),
        }

        add_other {
            expr: |_| op(Add, (), ()),
            want: TypeDef::new().fallible().scalar(Kind::Bytes | Kind::Integer | Kind::Float),
        }

        remainder {
            expr: |_| op(Rem, (), ()),
            want: TypeDef::new().fallible().scalar(Kind::Integer | Kind::Float),
        }

        subtract {
            expr: |_| op(Sub, (), ()),
            want: TypeDef::new().fallible().scalar(Kind::Integer | Kind::Float),
        }

        divide {
            expr: |_| op(Div, 10, 5),
            want: TypeDef::new().fallible().float(),
        }

        and {
            expr: |_| op(And, (), ()),
            want: TypeDef::new().boolean(),
        }

        equal {
            expr: |_| op(Eq, (), ()),
            want: TypeDef::new().boolean(),
        }

        not_equal {
            expr: |_| op(Ne, (), ()),
            want: TypeDef::new().boolean(),
        }

        greater {
            expr: |_| op(Gt, (), ()),
            want: TypeDef::new().fallible().boolean(),
        }

        greater_or_equal {
            expr: |_| op(Ge, (), ()),
            want: TypeDef::new().fallible().boolean(),
        }

        less {
            expr: |_| op(Lt, (), ()),
            want: TypeDef::new().fallible().boolean(),
        }

        less_or_equal {
            expr: |_| op(Le, (), ()),
            want: TypeDef::new().fallible().boolean(),
        }

        error_or_lhs_infallible {
            expr: |_| Op {
                lhs: Box::new(Literal::from("foo").into()),
                rhs: Box::new(Op {
                        lhs: Box::new(Literal::from("foo").into()),
                        rhs: Box::new(Literal::from(1).into()),
                        opcode: Div,
                    }.into(),
                ),
                opcode: Err,
            },
            want: TypeDef::new().bytes(),
        }

        error_or_rhs_infallible {
            expr: |_| Op {
                lhs: Box::new(Op {
                    lhs: Box::new(Literal::from("foo").into()),
                    rhs: Box::new(Literal::from(1).into()),
                    opcode: Div,
                }.into()),
                rhs: Box::new(Literal::from(true).into()),
                opcode: Err,
            },
            want: TypeDef::new().scalar(Kind::Float | Kind::Boolean),
        }

        error_or_fallible {
            expr: |_| Op {
                lhs: Box::new(Op {
                    lhs: Box::new(Literal::from("foo").into()),
                    rhs: Box::new(Literal::from(1).into()),
                    opcode: Div,
                }.into()),
                rhs: Box::new(Op {
                    lhs: Box::new(Literal::from(true).into()),
                    rhs: Box::new(Literal::from(1).into()),
                    opcode: Div,
                }.into()),
                opcode: Err,
            },
            want: TypeDef::new().fallible().float(),
        }

        error_or_nested_infallible {
            expr: |_| Op {
                lhs: Box::new(Op {
                    lhs: Box::new(Literal::from("foo").into()),
                    rhs: Box::new(Literal::from(1).into()),
                    opcode: Div,
                }.into()),
                rhs: Box::new(Op {
                    lhs: Box::new(Op {
                        lhs: Box::new(Literal::from(true).into()),
                        rhs: Box::new(Literal::from(1).into()),
                        opcode: Div,
                    }.into()),
                    rhs: Box::new(Literal::from("foo").into()),
                    opcode: Err,
                }.into()),
                opcode: Err,
            },
            want: TypeDef::new().scalar(Kind::Float | Kind::Bytes),
        }
    ];
}
