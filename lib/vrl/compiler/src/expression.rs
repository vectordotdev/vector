use crate::{Context, Span, State, TypeDef, Value};
use diagnostic::{DiagnosticError, Label, Note};
use dyn_clone::{clone_trait_object, DynClone};
use std::fmt;

mod array;
mod block;
mod function_argument;
mod group;
mod if_statement;
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

pub use array::Array;
pub use assignment::Assignment;
pub use block::Block;
pub use container::Container;
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
pub use query::Query;
pub use unary::Unary;
pub use variable::Variable;

pub type Resolved = Result<Value, ExpressionError>;

pub trait Expression: Send + Sync + fmt::Debug + DynClone {
    /// Resolve an expression to a concrete [`Value`].
    ///
    /// This method is executed at runtime.
    ///
    /// An expression is allowed to fail, which aborts the running program.
    fn resolve(&self, ctx: &mut Context) -> Resolved;

    /// Resolve an expression to its [`TypeDef`] type definition.
    ///
    /// This method is executed at compile-time.
    fn type_def(&self, state: &crate::State) -> TypeDef;

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
        }
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

// -----------------------------------------------------------------------------

#[derive(Debug, Default, PartialEq)]
pub struct ExpressionError {
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<Note>,
}

impl std::fmt::Display for ExpressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for ExpressionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl DiagnosticError for ExpressionError {
    fn code(&self) -> usize {
        0
    }

    fn message(&self) -> String {
        self.message.clone()
    }

    fn labels(&self) -> Vec<Label> {
        self.labels.clone()
    }

    fn notes(&self) -> Vec<Note> {
        self.notes.clone()
    }
}

impl From<String> for ExpressionError {
    fn from(message: String) -> Self {
        ExpressionError {
            message,
            ..Default::default()
        }
    }
}

impl From<&str> for ExpressionError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
