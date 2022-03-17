use std::collections::BTreeMap;
use std::fmt;

use diagnostic::{DiagnosticError, Label, Note};
use dyn_clone::{clone_trait_object, DynClone};

use crate::{vm, Context, Span, State, TypeDef, Value};

mod abort;
mod array;
mod block;
mod function_argument;
mod group;
mod if_statement;
mod levenstein;
mod noop;
mod not;
mod object;
mod op;
mod unary;
mod variable;

pub(crate) mod assignment;
pub(crate) mod container;
pub(crate) mod function_call;
pub(crate) mod literal;
pub(crate) mod predicate;
pub(crate) mod query;

pub use abort::Abort;
pub use array::Array;
pub use assignment::Assignment;
pub use block::Block;
pub use container::{Container, Variant};
pub use core::{ExpressionError, Resolved};
pub use function_argument::FunctionArgument;
pub use function_call::FunctionCall;
pub use group::Group;
pub use if_statement::IfStatement;
pub use literal::Literal;
pub use noop::Noop;
pub use not::Not;
pub use object::Object;
pub use op::Op;
pub use predicate::Predicate;
pub use query::{Query, Target};
pub use unary::Unary;
pub use variable::Variable;

pub trait Expression: Send + Sync + fmt::Debug + DynClone {
    /// Resolve an expression to a concrete [`Value`].
    ///
    /// This method is executed at runtime.
    ///
    /// An expression is allowed to fail, which aborts the running program.
    fn resolve(&self, ctx: &mut Context) -> Resolved;

    /// Compile the expression to bytecode that can be interpreted by the VM.
    fn compile_to_vm(
        &self,
        _vm: &mut vm::Vm,
        _state: &mut crate::state::Compiler,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Resolve an expression to a value without any context, if possible.
    ///
    /// This returns `Some` for static expressions, or `None` for dynamic expressions.
    fn as_value(&self) -> Option<Value> {
        None
    }

    /// Resolve an expression to its [`TypeDef`] type definition.
    ///
    /// This method is executed at compile-time.
    fn type_def(&self, state: &crate::State) -> TypeDef;

    /// Updates the state if necessary.
    /// By default it does nothing.
    fn update_state(&mut self, _state: &mut crate::State) -> Result<(), ExpressionError> {
        Ok(())
    }

    /// Format the expression into a consistent style.
    ///
    /// This defaults to not formatting, so that function implementations don't
    /// need to care about formatting (this is handled by the internal function
    /// call expression).
    fn format(&self) -> Option<String> {
        None
    }
}

clone_trait_object!(Expression);

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Container(Container),
    IfStatement(IfStatement),
    Op(Op),
    Assignment(Assignment),
    Query(Query),
    FunctionCall(FunctionCall),
    Variable(Variable),
    Noop(Noop),
    Unary(Unary),
    Abort(Abort),
}

impl Expr {
    pub fn as_str(&self) -> &str {
        use container::Variant::*;
        use Expr::*;

        match self {
            Literal(..) => "literal",
            Container(v) => match &v.variant {
                Group(..) => "group",
                Block(..) => "block",
                Array(..) => "array",
                Object(..) => "object",
            },
            IfStatement(..) => "if-statement",
            Op(..) => "operation",
            Assignment(..) => "assignment",
            Query(..) => "query",
            FunctionCall(..) => "function call",
            Variable(..) => "variable call",
            Noop(..) => "noop",
            Unary(..) => "unary operation",
            Abort(..) => "abort operation",
        }
    }

    pub fn as_literal(&self, keyword: &'static str) -> Result<Value, super::function::Error> {
        let literal = match self {
            Expr::Literal(literal) => Ok(literal.clone()),
            Expr::Variable(var) if var.value().is_some() => {
                match var.value().unwrap().clone().into() {
                    Expr::Literal(literal) => Ok(literal),
                    expr => Err(super::function::Error::UnexpectedExpression {
                        keyword,
                        expected: "literal",
                        expr,
                    }),
                }
            }
            expr => Err(super::function::Error::UnexpectedExpression {
                keyword,
                expected: "literal",
                expr: expr.clone(),
            }),
        }?;

        match literal.as_value() {
            Some(value) => Ok(value),
            None => Err(super::function::Error::UnexpectedExpression {
                keyword,
                expected: "literal",
                expr: self.clone(),
            }),
        }
    }

    pub fn as_enum(
        &self,
        keyword: &'static str,
        variants: Vec<Value>,
    ) -> Result<Value, super::function::Error> {
        let value = self.as_literal(keyword)?;
        variants.iter().find(|v| **v == value).cloned().ok_or(
            super::function::Error::InvalidEnumVariant {
                keyword,
                value,
                variants,
            },
        )
    }
}

impl Expression for Expr {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Expr::*;

        match self {
            Literal(v) => v.resolve(ctx),
            Container(v) => v.resolve(ctx),
            IfStatement(v) => v.resolve(ctx),
            Op(v) => v.resolve(ctx),
            Assignment(v) => v.resolve(ctx),
            Query(v) => v.resolve(ctx),
            FunctionCall(v) => v.resolve(ctx),
            Variable(v) => v.resolve(ctx),
            Noop(v) => v.resolve(ctx),
            Unary(v) => v.resolve(ctx),
            Abort(v) => v.resolve(ctx),
        }
    }

    fn as_value(&self) -> Option<Value> {
        use Expr::*;

        match self {
            Literal(v) => Expression::as_value(v),
            Container(v) => Expression::as_value(v),
            IfStatement(v) => Expression::as_value(v),
            Op(v) => Expression::as_value(v),
            Assignment(v) => Expression::as_value(v),
            Query(v) => Expression::as_value(v),
            FunctionCall(v) => Expression::as_value(v),
            Variable(v) => Expression::as_value(v),
            Noop(v) => Expression::as_value(v),
            Unary(v) => Expression::as_value(v),
            Abort(v) => Expression::as_value(v),
        }
    }

    fn type_def(&self, state: &State) -> TypeDef {
        use Expr::*;

        match self {
            Literal(v) => v.type_def(state),
            Container(v) => v.type_def(state),
            IfStatement(v) => v.type_def(state),
            Op(v) => v.type_def(state),
            Assignment(v) => v.type_def(state),
            Query(v) => v.type_def(state),
            FunctionCall(v) => v.type_def(state),
            Variable(v) => v.type_def(state),
            Noop(v) => v.type_def(state),
            Unary(v) => v.type_def(state),
            Abort(v) => v.type_def(state),
        }
    }

    fn compile_to_vm(
        &self,
        vm: &mut crate::vm::Vm,
        state: &mut crate::state::Compiler,
    ) -> Result<(), String> {
        use Expr::*;

        // Pass the call on to the contained expression.
        match self {
            Literal(v) => v.compile_to_vm(vm, state),
            Container(v) => v.compile_to_vm(vm, state),
            IfStatement(v) => v.compile_to_vm(vm, state),
            Op(v) => v.compile_to_vm(vm, state),
            Assignment(v) => v.compile_to_vm(vm, state),
            Query(v) => v.compile_to_vm(vm, state),
            FunctionCall(v) => v.compile_to_vm(vm, state),
            Variable(v) => v.compile_to_vm(vm, state),
            Noop(v) => v.compile_to_vm(vm, state),
            Unary(v) => v.compile_to_vm(vm, state),
            Abort(v) => v.compile_to_vm(vm, state),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Expr::*;

        match self {
            Literal(v) => v.fmt(f),
            Container(v) => v.fmt(f),
            IfStatement(v) => v.fmt(f),
            Op(v) => v.fmt(f),
            Assignment(v) => v.fmt(f),
            Query(v) => v.fmt(f),
            FunctionCall(v) => v.fmt(f),
            Variable(v) => v.fmt(f),
            Noop(v) => v.fmt(f),
            Unary(v) => v.fmt(f),
            Abort(v) => v.fmt(f),
        }
    }
}

// -----------------------------------------------------------------------------

impl From<Literal> for Expr {
    fn from(literal: Literal) -> Self {
        Expr::Literal(literal)
    }
}

impl From<Container> for Expr {
    fn from(container: Container) -> Self {
        Expr::Container(container)
    }
}

impl From<IfStatement> for Expr {
    fn from(if_statement: IfStatement) -> Self {
        Expr::IfStatement(if_statement)
    }
}

impl From<Op> for Expr {
    fn from(op: Op) -> Self {
        Expr::Op(op)
    }
}

impl From<Assignment> for Expr {
    fn from(assignment: Assignment) -> Self {
        Expr::Assignment(assignment)
    }
}

impl From<Query> for Expr {
    fn from(query: Query) -> Self {
        Expr::Query(query)
    }
}

impl From<FunctionCall> for Expr {
    fn from(function_call: FunctionCall) -> Self {
        Expr::FunctionCall(function_call)
    }
}

impl From<Variable> for Expr {
    fn from(variable: Variable) -> Self {
        Expr::Variable(variable)
    }
}

impl From<Noop> for Expr {
    fn from(noop: Noop) -> Self {
        Expr::Noop(noop)
    }
}

impl From<Unary> for Expr {
    fn from(unary: Unary) -> Self {
        Expr::Unary(unary)
    }
}

impl From<Abort> for Expr {
    fn from(abort: Abort) -> Self {
        Expr::Abort(abort)
    }
}

impl From<Value> for Expr {
    fn from(value: Value) -> Self {
        use Value::*;

        match value {
            Bytes(v) => Literal::from(v).into(),
            Integer(v) => Literal::from(v).into(),
            Float(v) => Literal::from(v).into(),
            Boolean(v) => Literal::from(v).into(),
            Object(v) => {
                let object = crate::expression::Object::from(
                    v.into_iter()
                        .map(|(k, v)| (k, v.into()))
                        .collect::<BTreeMap<_, _>>(),
                );

                Container::new(container::Variant::from(object)).into()
            }
            Array(v) => {
                let array = crate::expression::Array::from(
                    v.into_iter().map(Expr::from).collect::<Vec<_>>(),
                );

                Container::new(container::Variant::from(array)).into()
            }
            Timestamp(v) => Literal::from(v).into(),
            Regex(v) => Literal::from(v).into(),
            Null => Literal::from(()).into(),
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("unhandled error")]
    Fallible { span: Span },
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use Error::*;

        match self {
            Fallible { .. } => 100,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::*;

        match self {
            Fallible { span } => vec![
                Label::primary("expression can result in runtime error", span),
                Label::context("handle the error case to ensure runtime success", span),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use Error::*;

        match self {
            Fallible { .. } => vec![Note::SeeErrorDocs],
        }
    }
}
