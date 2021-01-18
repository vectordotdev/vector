use crate::{state, Object, Result, TypeDef, Value};
use std::convert::TryFrom;
use std::fmt;

mod argument;
mod arithmetic;
mod array;
pub(crate) mod assignment;
mod block;
pub(crate) mod function;
pub(crate) mod if_statement;
mod literal;
mod map;
mod noop;
mod not;
pub(crate) mod path;
mod variable;

pub use argument::Argument;
pub use arithmetic::Arithmetic;
pub use array::Array;
pub use assignment::{Assignment, Target};
pub use block::Block;
pub use function::Function;
pub use if_statement::IfStatement;
pub use literal::Literal;
pub use map::Map;
pub use noop::Noop;
pub use not::Not;
pub use path::Path;
pub use variable::Variable;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("unexpected expression")]
    Unexpected(#[from] ExprError),

    #[error(r#"error for function "{0}""#)]
    Function(String, #[source] function::Error),

    #[error("assignment error")]
    Assignment(#[from] assignment::Error),

    #[error("if-statement error")]
    IfStatement(#[from] if_statement::Error),
}

pub trait Expression: Send + Sync + fmt::Debug + dyn_clone::DynClone {
    /// Resolve an expression to a concrete [`Value`].
    ///
    /// This method is executed at runtime.
    ///
    /// An expression is allowed to fail, which aborts the running program.
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value>;

    /// Resolve an expression to its [`TypeDef`] type definition.
    ///
    /// This method is executed at compile-time.
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
        #[derive(Clone, PartialEq)]
        #[allow(clippy::large_enum_variant)] // TODO: investigate
        pub enum Expr {
            $($expr($expr)),+
        }

        impl Expr {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Expr::$expr(_) => stringify!($expr)),+
                }
            }

            pub fn boxed(self) -> Box<dyn Expression> {
                Box::new(self)
            }
        }

        impl fmt::Debug for Expr {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Expr::$expr(v) => v.fmt(f)),+
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
            fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
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
                    #[allow(unreachable_patterns)]
                    match expr {
                        Expr::$expr(v) => Ok(v),
                        Expr::Argument(v) => $expr::try_from(v.into_expr()),
                        _ => Err(Error::from(ExprError::$expr(expr.as_str()))),
                    }
                }
            }
        )+
    );
}

expression_dispatch![
    Argument,
    Arithmetic,
    Array,
    Assignment,
    Block,
    Function,
    IfStatement,
    Literal, // TODO: literal scalar
    Map,
    Noop,
    Not,
    Path,
    Variable,
];

impl<T: Into<Value>> From<T> for Expr {
    fn from(value: T) -> Self {
        let value = value.into();

        match value {
            Value::Array(array) => Array::from(array).into(),
            Value::Map(map) => Map::from(map).into(),
            _ => Literal::from(value).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::value;
    use value::Kind;

    #[test]
    fn test_contains() {
        let cases = vec![
            (true, Kind::all(), Kind::all()),
            (true, Kind::all(), Kind::Bytes),
            (true, Kind::all(), Kind::Integer),
            (true, Kind::all(), Kind::Float | Kind::Boolean),
            (true, Kind::all(), Kind::Map),
            (true, Kind::Bytes, Kind::Bytes),
            (true, Kind::Bytes, Kind::Bytes),
            (false, Kind::Bytes, Kind::Array),
            (false, Kind::Bytes, Kind::Integer),
            (false, Kind::Bytes, Kind::Integer | Kind::Float),
        ];

        for (expect, this, other) in cases {
            assert_eq!(this.contains(other), expect);
        }
    }

    #[test]
    fn test_merge() {
        let cases = vec![
            (Kind::all(), Kind::all(), Kind::all()),
            (Kind::all(), Kind::Integer | Kind::Bytes, Kind::all()),
            (Kind::Integer | Kind::Float, Kind::Integer, Kind::Float),
            (Kind::Integer, Kind::Integer, Kind::Integer),
            (
                Kind::Bytes | Kind::Integer | Kind::Float | Kind::Boolean,
                Kind::Integer | Kind::Bytes,
                Kind::Float | Kind::Boolean,
            ),
        ];

        for (expect, this, other) in cases {
            assert_eq!(this | other, expect);
        }
    }
}
