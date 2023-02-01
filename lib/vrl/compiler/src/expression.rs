use std::fmt;

use diagnostic::{DiagnosticMessage, Label, Note};
use dyn_clone::{clone_trait_object, DynClone};
use value::Value;

use crate::{Context, Span, TypeDef};

#[cfg(feature = "expr-abort")]
mod abort;
mod array;
mod block;
mod function_argument;
mod group;
#[cfg(feature = "expr-if_statement")]
mod if_statement;
mod levenstein;
mod noop;
#[cfg(feature = "expr-unary")]
mod not;
mod object;
#[cfg(feature = "expr-op")]
mod op;
#[cfg(feature = "expr-unary")]
mod unary;
mod variable;

#[cfg(feature = "expr-assignment")]
pub(crate) mod assignment;
pub(crate) mod container;
#[cfg(feature = "expr-function_call")]
pub(crate) mod function;
#[cfg(feature = "expr-function_call")]
pub(crate) mod function_call;
#[cfg(feature = "expr-literal")]
pub(crate) mod literal;
#[cfg(feature = "expr-if_statement")]
pub(crate) mod predicate;
#[cfg(feature = "expr-query")]
pub mod query;

pub use core::{ExpressionError, Resolved};

use crate::state::{TypeInfo, TypeState};
#[cfg(feature = "expr-abort")]
pub use abort::Abort;
pub use array::Array;
#[cfg(feature = "expr-assignment")]
pub use assignment::Assignment;
pub use block::Block;
pub use container::{Container, Variant};
#[cfg(feature = "expr-function_call")]
pub use function::FunctionExpression;
pub use function_argument::FunctionArgument;
#[cfg(feature = "expr-function_call")]
pub use function_call::FunctionCall;
pub use group::Group;
#[cfg(feature = "expr-if_statement")]
pub use if_statement::IfStatement;
#[cfg(feature = "expr-literal")]
pub use literal::Literal;
pub use noop::Noop;
#[cfg(feature = "expr-unary")]
pub use not::Not;
pub use object::Object;
#[cfg(feature = "expr-op")]
pub use op::Op;
#[cfg(feature = "expr-if_statement")]
pub use predicate::Predicate;
#[cfg(feature = "expr-query")]
pub use query::{Query, Target};
#[cfg(feature = "expr-unary")]
pub use unary::Unary;
pub use variable::Variable;

pub trait Expression: Send + Sync + fmt::Debug + DynClone {
    /// Resolve an expression to a concrete [`Value`].
    ///
    /// This method is executed at runtime.
    ///
    /// An expression is allowed to fail, which aborts the running program.
    fn resolve(&self, ctx: &mut Context) -> Resolved;

    /// Resolve an expression to a value without any context, if possible.
    ///
    /// This returns `Some` for static expressions, or `None` for dynamic expressions.
    fn as_value(&self) -> Option<Value> {
        None
    }

    /// Resolve an expression to its [`TypeDef`] type definition.
    /// This must be called with the _initial_ `TypeState`.
    ///
    /// Consider calling `type_info` instead if you want to capture changes in the type
    /// state from side-effects.
    fn type_def(&self, state: &TypeState) -> TypeDef {
        self.type_info(state).result
    }

    /// Calculates the type state after an expression resolves, including the expression result itself.
    /// This must be called with the _initial_ `TypeState`.
    ///
    /// Consider using `apply_type_info` instead if you want to just access
    /// the expr result type, while updating an existing state.
    fn type_info(&self, state: &TypeState) -> TypeInfo;

    /// Applies state changes from the expression to the given state, and
    /// returns the result type.
    fn apply_type_info(&self, state: &mut TypeState) -> TypeDef {
        let new_info = self.type_info(state);
        *state = new_info.state;
        new_info.result
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
    #[cfg(feature = "expr-literal")]
    Literal(Literal),
    Container(Container),
    #[cfg(feature = "expr-if_statement")]
    IfStatement(IfStatement),
    #[cfg(feature = "expr-op")]
    Op(Op),
    #[cfg(feature = "expr-assignment")]
    Assignment(Assignment),
    #[cfg(feature = "expr-query")]
    Query(Query),
    #[cfg(feature = "expr-function_call")]
    FunctionCall(FunctionCall),
    Variable(Variable),
    Noop(Noop),
    #[cfg(feature = "expr-unary")]
    Unary(Unary),
    #[cfg(feature = "expr-abort")]
    Abort(Abort),
}

impl Expr {
    pub fn as_str(&self) -> &str {
        use container::Variant::{Array, Block, Group, Object};
        use Expr::{
            Abort, Assignment, Container, FunctionCall, IfStatement, Literal, Noop, Op, Query,
            Unary, Variable,
        };

        match self {
            #[cfg(feature = "expr-literal")]
            Literal(..) => "literal",
            Container(v) => match &v.variant {
                Group(..) => "group",
                Block(..) => "block",
                Array(..) => "array",
                Object(..) => "object",
            },
            #[cfg(feature = "expr-if_statement")]
            IfStatement(..) => "if-statement",
            #[cfg(feature = "expr-op")]
            Op(..) => "operation",
            #[cfg(feature = "expr-assignment")]
            Assignment(..) => "assignment",
            #[cfg(feature = "expr-query")]
            Query(..) => "query",
            #[cfg(feature = "expr-function_call")]
            FunctionCall(..) => "function call",
            Variable(..) => "variable call",
            Noop(..) => "noop",
            #[cfg(feature = "expr-unary")]
            Unary(..) => "unary operation",
            #[cfg(feature = "expr-abort")]
            Abort(..) => "abort operation",
        }
    }

    #[cfg(feature = "expr-literal")]
    pub fn as_literal(&self, keyword: &'static str) -> Result<Value, super::function::Error> {
        let literal = match self {
            #[cfg(feature = "expr-literal")]
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

    #[cfg(not(feature = "expr-literal"))]
    pub fn as_literal(&self, keyword: &'static str) -> Result<Value, super::function::Error> {
        Err(super::function::Error::UnexpectedExpression {
            keyword,
            expected: "literal",
            expr: self.clone(),
        })
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
        use Expr::{
            Abort, Assignment, Container, FunctionCall, IfStatement, Literal, Noop, Op, Query,
            Unary, Variable,
        };

        match self {
            #[cfg(feature = "expr-literal")]
            Literal(v) => v.resolve(ctx),
            Container(v) => v.resolve(ctx),
            #[cfg(feature = "expr-if_statement")]
            IfStatement(v) => v.resolve(ctx),
            #[cfg(feature = "expr-op")]
            Op(v) => v.resolve(ctx),
            #[cfg(feature = "expr-assignment")]
            Assignment(v) => v.resolve(ctx),
            #[cfg(feature = "expr-query")]
            Query(v) => v.resolve(ctx),
            #[cfg(feature = "expr-function_call")]
            FunctionCall(v) => v.resolve(ctx),
            Variable(v) => v.resolve(ctx),
            Noop(v) => v.resolve(ctx),
            #[cfg(feature = "expr-unary")]
            Unary(v) => v.resolve(ctx),
            #[cfg(feature = "expr-abort")]
            Abort(v) => v.resolve(ctx),
        }
    }

    fn as_value(&self) -> Option<Value> {
        use Expr::{
            Abort, Assignment, Container, FunctionCall, IfStatement, Literal, Noop, Op, Query,
            Unary, Variable,
        };

        match self {
            #[cfg(feature = "expr-literal")]
            Literal(v) => Expression::as_value(v),
            Container(v) => Expression::as_value(v),
            #[cfg(feature = "expr-if_statement")]
            IfStatement(v) => Expression::as_value(v),
            #[cfg(feature = "expr-op")]
            Op(v) => Expression::as_value(v),
            #[cfg(feature = "expr-assignment")]
            Assignment(v) => Expression::as_value(v),
            #[cfg(feature = "expr-query")]
            Query(v) => Expression::as_value(v),
            #[cfg(feature = "expr-function_call")]
            FunctionCall(v) => Expression::as_value(v),
            Variable(v) => Expression::as_value(v),
            Noop(v) => Expression::as_value(v),
            #[cfg(feature = "expr-unary")]
            Unary(v) => Expression::as_value(v),
            #[cfg(feature = "expr-abort")]
            Abort(v) => Expression::as_value(v),
        }
    }

    fn type_info(&self, state: &TypeState) -> TypeInfo {
        use Expr::{
            Abort, Assignment, Container, FunctionCall, IfStatement, Literal, Noop, Op, Query,
            Unary, Variable,
        };

        match self {
            #[cfg(feature = "expr-literal")]
            Literal(v) => v.type_info(state),
            Container(v) => v.type_info(state),
            #[cfg(feature = "expr-if_statement")]
            IfStatement(v) => v.type_info(state),
            #[cfg(feature = "expr-op")]
            Op(v) => v.type_info(state),
            #[cfg(feature = "expr-assignment")]
            Assignment(v) => v.type_info(state),
            #[cfg(feature = "expr-query")]
            Query(v) => v.type_info(state),
            #[cfg(feature = "expr-function_call")]
            FunctionCall(v) => v.type_info(state),
            Variable(v) => v.type_info(state),
            Noop(v) => v.type_info(state),
            #[cfg(feature = "expr-unary")]
            Unary(v) => v.type_info(state),
            #[cfg(feature = "expr-abort")]
            Abort(v) => v.type_info(state),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Expr::{
            Abort, Assignment, Container, FunctionCall, IfStatement, Literal, Noop, Op, Query,
            Unary, Variable,
        };

        match self {
            #[cfg(feature = "expr-literal")]
            Literal(v) => v.fmt(f),
            Container(v) => v.fmt(f),
            #[cfg(feature = "expr-if_statement")]
            IfStatement(v) => v.fmt(f),
            #[cfg(feature = "expr-op")]
            Op(v) => v.fmt(f),
            #[cfg(feature = "expr-assignment")]
            Assignment(v) => v.fmt(f),
            #[cfg(feature = "expr-query")]
            Query(v) => v.fmt(f),
            #[cfg(feature = "expr-function_call")]
            FunctionCall(v) => v.fmt(f),
            Variable(v) => v.fmt(f),
            Noop(v) => v.fmt(f),
            #[cfg(feature = "expr-unary")]
            Unary(v) => v.fmt(f),
            #[cfg(feature = "expr-abort")]
            Abort(v) => v.fmt(f),
        }
    }
}

// -----------------------------------------------------------------------------

#[cfg(feature = "expr-literal")]
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

#[cfg(feature = "expr-if_statement")]
impl From<IfStatement> for Expr {
    fn from(if_statement: IfStatement) -> Self {
        Expr::IfStatement(if_statement)
    }
}

#[cfg(feature = "expr-op")]
impl From<Op> for Expr {
    fn from(op: Op) -> Self {
        Expr::Op(op)
    }
}

#[cfg(feature = "expr-assignment")]
impl From<Assignment> for Expr {
    fn from(assignment: Assignment) -> Self {
        Expr::Assignment(assignment)
    }
}

#[cfg(feature = "expr-query")]
impl From<Query> for Expr {
    fn from(query: Query) -> Self {
        Expr::Query(query)
    }
}

#[cfg(feature = "expr-function_call")]
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

#[cfg(feature = "expr-unary")]
impl From<Unary> for Expr {
    fn from(unary: Unary) -> Self {
        Expr::Unary(unary)
    }
}

#[cfg(feature = "expr-abort")]
impl From<Abort> for Expr {
    fn from(abort: Abort) -> Self {
        Expr::Abort(abort)
    }
}

#[cfg(feature = "expr-literal")]
impl From<Value> for Expr {
    fn from(value: Value) -> Self {
        use std::collections::BTreeMap;

        use value::Value::{Array, Boolean, Bytes, Float, Integer, Null, Object, Regex, Timestamp};

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

#[cfg(not(feature = "expr-literal"))]
impl From<Value> for Expr {
    fn from(_: Value) -> Self {
        Self::Noop(Noop)
    }
}

// -----------------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("unhandled error")]
    Fallible { span: Span },

    #[error("expression type unavailable")]
    Missing { span: Span, feature: &'static str },
}

impl DiagnosticMessage for Error {
    fn code(&self) -> usize {
        use Error::{Fallible, Missing};

        match self {
            Fallible { .. } => 100,
            Missing { .. } => 900,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::{Fallible, Missing};

        match self {
            Fallible { span } => vec![
                Label::primary("expression can result in runtime error", span),
                Label::context("handle the error case to ensure runtime success", span),
            ],
            Missing { span, feature } => vec![
                Label::primary("expression type is disabled in this version of vrl", span),
                Label::context(
                    format!("build vrl using the `{feature}` feature to enable it"),
                    span,
                ),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use Error::{Fallible, Missing};

        match self {
            Fallible { .. } => vec![Note::SeeErrorDocs],
            Missing { .. } => vec![],
        }
    }
}
