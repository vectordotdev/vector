use std::fmt;

use diagnostic::{DiagnosticError, Label, Note, Span, Urls};

use crate::{
    expression::{self, Expr, Noop, Resolved},
    parser::{ast, Node},
    value,
    vm::OpCode,
    Context, Expression, State, TypeDef, Value,
};

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

        if matches!(opcode, Eq | Ne | Lt | Le | Gt | Ge) {
            if let Expr::Op(op) = &lhs {
                if matches!(op.opcode, Eq | Ne | Lt | Le | Gt | Ge) {
                    return Err(Error::ChainedComparison { span: op_span });
                }
            }
        }

        if let ast::Opcode::Err = opcode {
            if lhs_type_def.is_infallible() {
                return Err(Error::UnnecessaryCoalesce {
                    lhs_span,
                    rhs_span,
                    op_span,
                });
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

        let lhs_kind = lhs_def.kind();
        let rhs_kind = rhs_def.kind();

        match self.opcode {
            // ok/err ?? ok
            Err if rhs_def.is_infallible() => lhs_def.merge(rhs_def).infallible(),

            // ... ?? ...
            Err => lhs_def.merge(rhs_def),

            // null || ...
            Or if lhs_kind.is_null() => rhs_def,

            // not null || ...
            Or if !(lhs_kind.contains(K::Null) || lhs_kind.contains(K::Boolean)) => lhs_def,

            // ... || ...
            Or if !lhs_kind.is_boolean() => {
                // We can remove Null from the lhs since we know that if the lhs is Null
                // we will be taking the rhs and only the rhs type_def will then be relevant.
                (lhs_def - K::Null).merge(rhs_def)
            }

            Or => lhs_def.merge(rhs_def),

            // ... | ...
            Merge => lhs_def.merge(rhs_def),

            // null && ...
            And if lhs_kind.is_null() => rhs_def
                .fallible_unless(K::Null | K::Boolean)
                .scalar(K::Boolean),

            // ... && ...
            And => lhs_def
                .fallible_unless(K::Null | K::Boolean)
                .merge(rhs_def.fallible_unless(K::Null | K::Boolean))
                .scalar(K::Boolean),

            // ... == ...
            // ... != ...
            Eq | Ne => lhs_def.merge(rhs_def).boolean(),

            // "b" >  "a"
            // "a" >= "a"
            // "a" <  "b"
            // "b" <= "b"
            Gt | Ge | Lt | Le if lhs_def.is_bytes() && rhs_def.is_bytes() => {
                lhs_def.merge(rhs_def).boolean()
            }

            // ... >  ...
            // ... >= ...
            // ... <  ...
            // ... <= ...
            Gt | Ge | Lt | Le => lhs_def
                .fallible_unless(K::Integer | K::Float)
                .merge(rhs_def.fallible_unless(K::Integer | K::Float))
                .scalar(K::Boolean),

            // ... / ...
            Div => TypeDef::new().fallible().float(),

            // "bar" + ...
            // ... + "bar"
            Add if lhs_kind.is_bytes() || rhs_kind.is_bytes() => lhs_def
                .fallible_unless(K::Bytes | K::Null)
                .merge(rhs_def.fallible_unless(K::Bytes | K::Null))
                .scalar(K::Bytes),

            // ... + 1.0
            // ... - 1.0
            // ... * 1.0
            // ... % 1.0
            // 1.0 + ...
            // 1.0 - ...
            // 1.0 * ...
            // 1.0 % ...
            Add | Sub | Mul | Rem if lhs_kind.is_float() || rhs_kind.is_float() => lhs_def
                .fallible_unless(K::Integer | K::Float)
                .merge(rhs_def.fallible_unless(K::Integer | K::Float))
                .scalar(K::Float),

            // 1 + 1
            // 1 - 1
            // 1 * 1
            // 1 % 1
            Add | Sub | Mul | Rem if lhs_kind.is_integer() && rhs_kind.is_integer() => {
                lhs_def.merge(rhs_def).scalar(K::Integer)
            }

            // "bar" * 1
            Mul if lhs_kind.is_bytes() && rhs_kind.is_integer() => {
                lhs_def.merge(rhs_def).scalar(K::Bytes)
            }

            // 1 * "bar"
            Mul if lhs_kind.is_integer() && rhs_kind.is_bytes() => {
                lhs_def.merge(rhs_def).scalar(K::Bytes)
            }

            // ... + ...
            // ... * ...
            Add | Mul => lhs_def
                .merge(rhs_def)
                .fallible()
                .scalar(K::Bytes | K::Integer | K::Float),

            // ... - ...
            // ... % ...
            Sub | Rem => lhs_def
                .merge(rhs_def)
                .fallible()
                .scalar(K::Integer | K::Float),
        }
    }

    fn compile_to_vm(&self, vm: &mut crate::vm::Vm) -> Result<(), String> {
        self.lhs.compile_to_vm(vm)?;

        // Note, not all opcodes want the RHS evaluated straight away, so we
        // only compile the rhs in each branch as necessary.
        match self.opcode {
            ast::Opcode::Mul => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::Multiply);
            }
            ast::Opcode::Div => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::Divide);
            }
            ast::Opcode::Add => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::Add);
            }
            ast::Opcode::Sub => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::Subtract);
            }
            ast::Opcode::Rem => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::Rem);
            }
            ast::Opcode::Or => {
                // Or is rewritten as an if statement to allow short circuiting.
                let if_jump = vm.emit_jump(OpCode::JumpIfTruthy);
                vm.write_opcode(OpCode::Pop);
                self.rhs.compile_to_vm(vm)?;
                vm.patch_jump(if_jump);
            }
            ast::Opcode::And => {
                // And is rewritten as an if statement to allow short circuiting
                let if_jump = vm.emit_jump(OpCode::JumpIfFalse);
                vm.write_opcode(OpCode::Pop);
                self.rhs.compile_to_vm(vm)?;
                vm.patch_jump(if_jump);
            }
            ast::Opcode::Err => {
                // Err is rewritten as an if statement to allow short circuiting
                let if_jump = vm.emit_jump(OpCode::JumpIfNotErr);
                vm.write_opcode(OpCode::ClearError);
                self.rhs.compile_to_vm(vm)?;
                vm.patch_jump(if_jump);
            }
            ast::Opcode::Ne => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::NotEqual);
            }
            ast::Opcode::Eq => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::Equal);
            }
            ast::Opcode::Ge => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::GreaterEqual);
            }
            ast::Opcode::Gt => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::Greater);
            }
            ast::Opcode::Le => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::LessEqual);
            }
            ast::Opcode::Lt => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::Less);
            }
            ast::Opcode::Merge => {
                self.rhs.compile_to_vm(vm)?;
                vm.write_opcode(OpCode::Merge);
            }
        };
        Ok(())
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
    UnnecessaryCoalesce {
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
            UnnecessaryCoalesce { .. } => 651,
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
            UnnecessaryCoalesce {
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
    use std::convert::TryInto;

    use ast::Opcode::*;
    use ordered_float::NotNan;

    use super::*;
    use crate::{
        expression::{Block, IfStatement, Literal, Predicate},
        test_type_def,
        value::Kind,
    };

    fn op(
        opcode: ast::Opcode,
        lhs: impl TryInto<Literal> + fmt::Debug + Clone,
        rhs: impl TryInto<Literal> + fmt::Debug + Clone,
    ) -> Op {
        use std::result::Result::Err;

        let lhs = match lhs.clone().try_into() {
            Ok(v) => v,
            Err(_) => panic!("not a valid lhs expression: {:?}", lhs),
        };

        let rhs = match rhs.clone().try_into() {
            Ok(v) => v,
            Err(_) => panic!("not a valid rhs expression: {:?}", rhs),
        };

        Op {
            lhs: Box::new(lhs.into()),
            rhs: Box::new(rhs.into()),
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

        remainder_integer {
            expr: |_| op(Rem, 5, 5),
            want: TypeDef::new().infallible().integer(),
        }

        remainder_float {
            expr: |_| op(Rem, 5.0, 5.0),
            want: TypeDef::new().infallible().float(),
        }

        remainder_mixed {
            expr: |_| op(Rem, 5, 5.0),
            want: TypeDef::new().infallible().float(),
        }

        remainder_other {
            expr: |_| op(Rem, 5, ()),
            want: TypeDef::new().fallible().integer().add_float(),
        }

        subtract_integer {
            expr: |_| op(Sub, 1, 1),
            want: TypeDef::new().infallible().integer(),
        }

        subtract_float {
            expr: |_| op(Sub, 1.0, 1.0),
            want: TypeDef::new().infallible().float(),
        }

        subtract_mixed {
            expr: |_| op(Sub, 1, 1.0),
            want: TypeDef::new().infallible().float(),
        }

        subtract_other {
            expr: |_| op(Sub, 1, ()),
            want: TypeDef::new().fallible().integer().add_float(),
        }

        divide_integer {
            expr: |_| op(Div, 1, 1),
            want: TypeDef::new().fallible().float(),
        }

        divide_float {
            expr: |_| op(Div, 1.0, 1.0),
            want: TypeDef::new().fallible().float(),
        }

        divide_mixed {
            expr: |_| op(Div, 1, 1.0),
            want: TypeDef::new().fallible().float(),
        }

        divide_other {
            expr: |_| op(Div, 1.0, ()),
            want: TypeDef::new().fallible().float(),
        }

        and_null {
            expr: |_| op(And, (), ()),
            want: TypeDef::new().infallible().boolean(),
        }

        and_boolean {
            expr: |_| op(And, true, true),
            want: TypeDef::new().infallible().boolean(),
        }

        and_mixed {
            expr: |_| op(And, (), true),
            want: TypeDef::new().infallible().boolean(),
        }

        and_other {
            expr: |_| op(And, (), "bar"),
            want: TypeDef::new().fallible().boolean(),
        }

        equal {
            expr: |_| op(Eq, (), ()),
            want: TypeDef::new().infallible().boolean(),
        }

        not_equal {
            expr: |_| op(Ne, (), "foo"),
            want: TypeDef::new().infallible().boolean(),
        }

        greater_integer {
            expr: |_| op(Gt, 1, 1),
            want: TypeDef::new().infallible().boolean(),
        }

        greater_float {
            expr: |_| op(Gt, 1.0, 1.0),
            want: TypeDef::new().infallible().boolean(),
        }

        greater_mixed {
            expr: |_| op(Gt, 1, 1.0),
            want: TypeDef::new().infallible().boolean(),
        }

        greater_bytes {
            expr: |_| op(Gt, "c", "b"),
            want: TypeDef::new().infallible().boolean(),
        }

        greater_other {
            expr: |_| op(Gt, 1, "foo"),
            want: TypeDef::new().fallible().boolean(),
        }

        greater_or_equal_integer {
            expr: |_| op(Ge, 1, 1),
            want: TypeDef::new().infallible().boolean(),
        }

        greater_or_equal_float {
            expr: |_| op(Ge, 1.0, 1.0),
            want: TypeDef::new().infallible().boolean(),
        }

        greater_or_equal_mixed {
            expr: |_| op(Ge, 1, 1.0),
            want: TypeDef::new().infallible().boolean(),
        }

        greater_or_equal_bytes {
            expr: |_| op(Ge, "foo", "foo"),
            want: TypeDef::new().infallible().boolean(),
        }

        greater_or_equal_other {
            expr: |_| op(Ge, 1, "foo"),
            want: TypeDef::new().fallible().boolean(),
        }

        less_integer {
            expr: |_| op(Lt, 1, 1),
            want: TypeDef::new().infallible().boolean(),
        }

        less_float {
            expr: |_| op(Lt, 1.0, 1.0),
            want: TypeDef::new().infallible().boolean(),
        }

        less_mixed {
            expr: |_| op(Lt, 1, 1.0),
            want: TypeDef::new().infallible().boolean(),
        }

        less_bytes {
            expr: |_| op(Lt, "bar", "foo"),
            want: TypeDef::new().infallible().boolean(),
        }

        less_other {
            expr: |_| op(Lt, 1, "foo"),
            want: TypeDef::new().fallible().boolean(),
        }

        less_or_equal_integer {
            expr: |_| op(Le, 1, 1),
            want: TypeDef::new().infallible().boolean(),
        }

        less_or_equal_float {
            expr: |_| op(Le, 1.0, 1.0),
            want: TypeDef::new().infallible().boolean(),
        }

        less_or_equal_mixed {
            expr: |_| op(Le, 1, 1.0),
            want: TypeDef::new().infallible().boolean(),
        }

        less_or_equal_bytes {
            expr: |_| op(Le, "bar", "bar"),
            want: TypeDef::new().infallible().boolean(),
        }

        less_or_equal_other {
            expr: |_| op(Le, 1, "baz"),
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
