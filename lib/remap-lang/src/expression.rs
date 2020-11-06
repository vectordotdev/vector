use crate::{CompilerState, Object, Result, State, Value, ValueConstraint, ValueKind};

pub(super) mod arithmetic;
pub(super) mod assignment;
mod block;
pub(super) mod function;
pub(super) mod if_statement;
mod literal;
mod noop;
pub(super) mod not;
pub(super) mod path;
pub(super) mod variable;

pub(super) use arithmetic::Arithmetic;
pub(super) use assignment::{Assignment, Target};
pub(super) use block::Block;
pub(super) use function::Function;
pub(super) use if_statement::IfStatement;
pub(super) use not::Not;
pub(super) use variable::Variable;

pub use literal::Literal;
pub use noop::Noop;
pub use path::Path;

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

pub trait Expression: Send + Sync + std::fmt::Debug + dyn_clone::DynClone {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>>;
    fn resolves_to(&self, state: &CompilerState) -> ValueConstraint;
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
        pub(crate) enum Expr {
            $($expr($expr)),+
        }

        impl Expression for Expr {
            fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
                match self {
                    $(Expr::$expr(expression) => expression.execute(state, object)),+
                }
            }

            fn resolves_to(&self, state: &CompilerState) -> ValueConstraint {
                match self {
                    $(Expr::$expr(expression) => expression.resolves_to(state)),+
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
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains() {
        use ValueConstraint::*;
        use ValueKind::*;

        let cases = vec![
            (true, Any, Any),
            (true, Any, Exact(String)),
            (true, Any, Exact(Integer)),
            (true, Any, OneOf(vec![Float, Boolean])),
            (true, Any, OneOf(vec![Map])),
            (false, Any, Maybe(Box::new(Any))),
            (true, Exact(String), Exact(String)),
            (true, Exact(String), OneOf(vec![String])),
            (false, Exact(String), Exact(Array)),
            (false, Exact(String), OneOf(vec![Integer])),
            (false, Exact(String), OneOf(vec![Integer, Float])),
            (false, Exact(String), Maybe(Box::new(Any))),
        ];

        for (expect, this, other) in cases {
            assert_eq!(this.contains(&other), expect);
        }
    }

    #[test]
    fn test_merge() {
        use ValueConstraint::*;
        use ValueKind::*;

        let cases = vec![
            (Any, Any, Any),
            (Maybe(Box::new(Any)), Maybe(Box::new(Any)), Any),
            (Maybe(Box::new(Any)), Any, Maybe(Box::new(Any))),
            (Any, OneOf(vec![Integer, String]), Any),
            (OneOf(vec![Integer, Float]), Exact(Integer), Exact(Float)),
            (Exact(Integer), Exact(Integer), Exact(Integer)),
            (
                Maybe(Box::new(Exact(Integer))),
                Maybe(Box::new(Exact(Integer))),
                Exact(Integer),
            ),
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
