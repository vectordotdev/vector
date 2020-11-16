use crate::{state, Object, Result, TypeDef, Value};
use std::convert::TryFrom;

mod argument;
mod arithmetic;
mod assignment;
mod block;
pub(crate) mod function;
mod if_statement;
mod literal;
mod noop;
mod not;
pub(crate) mod path;
mod variable;

pub use argument::Argument;
pub use arithmetic::Arithmetic;
pub use assignment::{Assignment, Target};
pub use block::Block;
pub use function::Function;
pub use if_statement::IfStatement;
pub use literal::Literal;
pub use noop::Noop;
pub use not::Not;
pub use path::Path;
pub use variable::Variable;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("expected expression, got none")]
    Missing,

    #[error("unexpected expression")]
    Unexpected(#[from] ExprError),

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

pub trait Expression: Send + Sync + std::fmt::Debug + dyn_clone::DynClone {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object)
        -> Result<Option<Value>>;
    fn type_def(&self, state: &state::Compiler) -> TypeDef;
}

dyn_clone::clone_trait_object!(Expression);

macro_rules! expression_dispatch {
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
        #[derive(Debug, Clone)]
        pub enum Expr {
            $($expr($expr)),+
        }

        impl Expr {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Expr::$expr(_) => stringify!($expr)),+
                }
            }
        }

        #[derive(thiserror::Error, Clone, Debug, PartialEq)]
        pub enum ExprError {
            $(
                #[error(r#"expected {}, got {0}"#, stringify!($expr))]
                $expr(&'static str)
            ),+
        }

        impl Expression for Expr {
            fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Option<Value>> {
                match self {
                    $(Expr::$expr(expression) => expression.execute(state, object)),+
                }
            }

            fn type_def(&self, state: &state::Compiler) -> TypeDef {
                match self {
                    $(Expr::$expr(expression) => expression.type_def(state)),+
                }
            }
        }

        $(
            impl From<$expr> for Expr {
                fn from(expression: $expr) -> Self {
                    Expr::$expr(expression)
                }
            }

            impl TryFrom<Expr> for $expr {
                type Error = Error;

                fn try_from(expr: Expr) -> std::result::Result<Self, Self::Error> {
                    match expr {
                        Expr::$expr(v) => Ok(v),
                        _ => Err(Error::from(ExprError::$expr(expr.as_str()))),
                    }
                }
            }
        )+
    );
}

expression_dispatch![
    Arithmetic,
    Assignment,
    Block,
    Function,
    IfStatement,
    Literal,
    Noop,
    Not,
    Path,
    Variable,
    Argument,
];

#[cfg(test)]
mod tests {
    use crate::value;

    #[test]
    fn test_contains() {
        use value::Constraint::*;
        use value::Kind::*;

        let cases = vec![
            (true, Any, Any),
            (true, Any, Exact(String)),
            (true, Any, Exact(Integer)),
            (true, Any, OneOf(vec![Float, Boolean])),
            (true, Any, OneOf(vec![Map])),
            (true, Exact(String), Exact(String)),
            (true, Exact(String), OneOf(vec![String])),
            (false, Exact(String), Exact(Array)),
            (false, Exact(String), OneOf(vec![Integer])),
            (false, Exact(String), OneOf(vec![Integer, Float])),
        ];

        for (expect, this, other) in cases {
            assert_eq!(this.contains(&other), expect);
        }
    }

    #[test]
    fn test_merge() {
        use value::Constraint::*;
        use value::Kind::*;

        let cases = vec![
            (Any, Any, Any),
            (Any, OneOf(vec![Integer, String]), Any),
            (OneOf(vec![Integer, Float]), Exact(Integer), Exact(Float)),
            (Exact(Integer), Exact(Integer), Exact(Integer)),
            (
                OneOf(vec![String, Integer, Float, Boolean]),
                OneOf(vec![Integer, String]),
                OneOf(vec![Float, Boolean]),
            ),
        ];

        for (expect, this, other) in cases {
            assert_eq!(this.merge(&other), expect);
        }
    }
}
