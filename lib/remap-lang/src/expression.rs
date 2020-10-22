use crate::{Object, Result, State, Value};

pub(super) mod arithmetic;
pub(super) mod assignment;
pub(super) mod function;
pub(super) mod if_statement;
mod literal;
mod noop;
pub(super) mod not;
pub(super) mod path;
pub(super) mod variable;

pub(super) use arithmetic::Arithmetic;
pub(super) use assignment::{Assignment, Target};
pub(super) use function::Function;
pub(super) use if_statement::IfStatement;
pub(super) use literal::Literal;
pub(super) use noop::Noop;
pub(super) use not::Not;
pub(super) use path::Path;
pub(super) use variable::Variable;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("expected expression, got none")]
    Missing,

    #[error(r#"error for function "{0}""#)]
    Function(String, #[source] function::Error),

    #[error("assignment error")]
    Assignment(#[from] assignment::Error),

    #[error("path error")]
    Path(#[from] path::Error),

    #[error("not operation error")]
    Not(#[from] not::Error),

    #[error("if-statement error")]
    IfStatement(#[from] if_statement::Error),

    #[error("variable error")]
    Variable(#[from] variable::Error),
}

pub trait Expression: std::fmt::Debug {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>>;

    // TODO:
    //
    // 1. use `execute_safe` instead
    // 2. override this in `Assignment` to error if the path doesn't exist, but
    //    allow the value not to exist (resulting in no assignment happening).
    fn execute_infallible(&self, state: &mut State, object: &mut dyn Object) -> Option<Value> {
        self.execute(state, object).ok().flatten()
    }
}

macro_rules! enum_dispatch {
    ($($expr:tt),+ $(,)?) => (
        /// The list of implemented expressions.
        ///
        /// This enum serves the purpose that the `enum_dispatch` crate usually
        /// provides:
        ///
        /// It allows using concrete expression types instead of `Box<dyn Expression>`
        /// trait objects, to improve runtime performance.
        ///
        /// Any expression that stores other expressions internally will still
        /// have to box this enum, to avoid infinite recursion.
        #[derive(Debug)]
        pub(crate) enum Expr {
            $($expr($expr)),+
        }

        impl Expression for Expr {
            fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
                match self {
                    $(Expr::$expr(expression) => expression.execute(state, object)),+
                }
            }
        }

        $(
            impl From<$expr> for Expr {
                fn from(expression: $expr) -> Self {
                    Expr::$expr(expression)
                }
            }
        )+
    );
}

enum_dispatch![
    Arithmetic,
    Assignment,
    Function,
    IfStatement,
    Literal,
    Noop,
    Not,
    Path,
    Variable,
];
