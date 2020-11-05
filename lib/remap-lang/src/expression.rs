use crate::{CompilerState, Object, Result, State, Value, ValueKind};

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

/// What kind of value an expression is going to resolve to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolveKind {
    /// The expression can resolve to any value at runtime.
    ///
    /// This applies to any [`Object`] value that hasn't been coerced yet.
    Any,

    /// The expressions resolves to one of the defined [`ValueKind`]s.
    OneOf(Vec<ValueKind>),

    /// The expression resolves to an exact value.
    Exact(ValueKind),

    /// If the expression succeeds, it might resolve to a value, but doesn't
    /// have to.
    ///
    /// For example, and if-statement without an else-condition can resolve to
    /// nothing if the if-condition does not match.
    Maybe(Box<ResolveKind>),
}

impl ResolveKind {
    pub fn is_exact(&self) -> bool {
        matches!(self, ResolveKind::Exact(_))
    }

    pub fn is_any(&self) -> bool {
        matches!(self, ResolveKind::Any)
    }

    pub fn is_maybe(&self) -> bool {
        matches!(self, ResolveKind::Maybe(_))
    }

    pub fn value_kinds(&self) -> Vec<ValueKind> {
        use ResolveKind::*;

        match self {
            Any => ValueKind::all(),
            OneOf(v) => v.clone(),
            Exact(v) => vec![*v],
            Maybe(v) => v.value_kinds(),
        }
    }
}

pub trait Expression: Send + Sync + std::fmt::Debug + dyn_clone::DynClone {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>>;
    fn resolves_to(&self, state: &CompilerState) -> ResolveKind;
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

            fn resolves_to(&self, state: &CompilerState) -> ResolveKind {
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
