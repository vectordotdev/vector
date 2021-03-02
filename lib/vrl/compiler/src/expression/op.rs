use crate::expression::{self, Expr, Noop, Resolved};
use crate::parser::{ast, Node};
use crate::{value, Context, Expression, State, TypeDef, Value};
use diagnostic::{DiagnosticError, Label, Note, Span, Urls};
use std::fmt;

#[derive(Clone, PartialEq)]
pub struct Op {
    pub(crate) lhs: Box<Expr>,
    pub(crate) rhs: Box<Expr>,
    pub(crate) opcode: ast::Opcode,
}

impl Op {
    pub fn new(
        lhs: Node<Expr>,
        opcode: Node<ast::Opcode>,
        rhs: Node<Expr>,
        state: &State,
    ) -> Result<Self, Error> {
        use ast::Opcode::{Eq, Ge, Gt, Le, Lt, Ne};

        let (op_span, opcode) = opcode.take();

        let (lhs_span, lhs) = lhs.take();
        let lhs_type_def = lhs.type_def(state);

        let (rhs_span, rhs) = rhs.take();
        let rhs_type_def = rhs.type_def(state);

        if matches!(opcode, Eq | Ne | Lt | Le | Gt | Ge) {
            if let Expr::Op(op) = &lhs {
                if matches!(op.opcode, Eq | Ne | Lt | Le | Gt | Ge) {
                    return Err(Error::ChainedComparison { span: op_span });
                }
            }
        }

        if let ast::Opcode::Err = opcode {
            if lhs_type_def.is_infallible() {
                return Err(Error::ErrInfallible {
                    lhs_span,
                    rhs_span,
                    op_span,
                });
            } else if rhs_type_def.is_fallible() {
                return Err(expression::Error::Fallible { span: rhs_span }.into());
            }
        }

        if let ast::Opcode::Merge = opcode {
            if !(lhs.type_def(state).is_object() && rhs.type_def(state).is_object()) {
                return Err(Error::MergeNonObjects {
                    lhs_span: if lhs.type_def(state).is_object() {
                        None
                    } else {
                        Some(lhs_span)
                    },
                    rhs_span: if rhs.type_def(state).is_object() {
                        None
                    } else {
                        Some(rhs_span)
                    },
                });
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
            Merge => lhs?.try_merge(rhs()?),
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

        match self.opcode {
            // ok/err ?? ok
            Err if rhs_def.is_infallible() => merged_def.infallible(),

            // ... ?? ...
            Err => merged_def,

            // null || ...
            Or if lhs_kind.is_null() => rhs_def,

            // not null || ...
            Or if !lhs_kind.contains(K::Null) => lhs_def,

            // ... || ...
            Or if !lhs_kind.is_boolean() => {
                // We can remove Null from the lhs since we know that if the lhs is Null
                // we will be taking the rhs and only the rhs type_def will then be relevant.
                (lhs_def - K::Null).merge(rhs_def)
            }

            Or => merged_def,

            // ... | ...
            Merge => merged_def,

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
                merged_def.scalar(K::Integer)
            }

            // "bar" * 1
            Mul if lhs_kind.is_bytes() && rhs_kind.is_integer() => merged_def.scalar(K::Bytes),

            // 1 * "bar"
            Mul if lhs_kind.is_integer() && rhs_kind.is_bytes() => merged_def.scalar(K::Bytes),

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
    #[error("comparison operators can't be chained together")]
    ChainedComparison { span: Span },

    #[error("unnecessary error coalescing operation")]
    ErrInfallible {
        lhs_span: Span,
        rhs_span: Span,
        op_span: Span,
    },

    #[error("only objects can be merged")]
    MergeNonObjects {
        lhs_span: Option<Span>,
        rhs_span: Option<Span>,
    },

    #[error("fallible operation")]
    Expr(#[from] expression::Error),
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use Error::*;

        match self {
            ChainedComparison { .. } => 650,
            ErrInfallible { .. } => 651,
            MergeNonObjects { .. } => 652,
            Expr(err) => err.code(),
        }
    }

    fn message(&self) -> String {
        use Error::*;

        match self {
            Expr(err) => err.message(),
            err => err.to_string(),
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::*;

        match self {
            ChainedComparison { span } => vec![Label::primary("", span)],
            ErrInfallible {
                lhs_span,
                rhs_span,
                op_span,
            } => vec![
                Label::primary("this expression can't fail", lhs_span),
                Label::context("this expression never resolves", rhs_span),
                Label::context("remove this error coalescing operation", op_span),
            ],
            MergeNonObjects { lhs_span, rhs_span } => {
                let mut labels = Vec::new();
                if let Some(lhs_span) = lhs_span {
                    labels.push(Label::primary(
                        "this expression must resolve to an object",
                        lhs_span,
                    ));
                }
                if let Some(rhs_span) = rhs_span {
                    labels.push(Label::primary(
                        "this expression must resolve to an object",
                        rhs_span,
                    ));
                }

                labels
            }
            Expr(err) => err.labels(),
        }
    }

    fn notes(&self) -> Vec<Note> {
        use Error::*;

        match self {
            ChainedComparison { .. } => vec![Note::SeeDocs(
                "comparisons".to_owned(),
                Urls::expression_docs_url("#comparison"),
            )],
            Expr(err) => err.notes(),
            _ => vec![],
        }
    }
}

// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::{Block, IfStatement, Literal, Predicate};
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
            want: TypeDef::new().fallible().bytes().add_integer().add_float(),
        }

        remainder {
            expr: |_| op(Rem, (), ()),
            want: TypeDef::new().fallible().integer().add_float(),
        }

        subtract {
            expr: |_| op(Sub, (), ()),
            want: TypeDef::new().fallible().integer().add_float(),
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
            want: TypeDef::new().float().add_boolean(),
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
            want: TypeDef::new().float().add_bytes(),
        }

        or_nullable {
            expr: |_| Op {
                lhs: Box::new(
                    IfStatement {
                        predicate: Predicate::new_unchecked(vec![Literal::from(true).into()]),
                        consequent: Block::new(vec![Literal::from("string").into()]),
                        alternative: None,
                    }.into()),
                rhs: Box::new(Literal::from("another string").into()),
                opcode: Or,
            },
            want: TypeDef::new().bytes(),
        }

        or_not_nullable {
            expr: |_| Op {
                lhs: Box::new(
                    IfStatement {
                        predicate: Predicate::new_unchecked(vec![Literal::from(true).into()]),
                        consequent: Block::new(vec![Literal::from("string").into()]),
                        alternative:  Some(Block::new(vec![Literal::from(42).into()]))
                }.into()),
                rhs: Box::new(Literal::from("another string").into()),
                opcode: Or,
            },
            want: TypeDef::new().bytes().add_integer(),
        }
    ];
}
