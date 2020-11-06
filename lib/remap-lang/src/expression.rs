use crate::{CompilerState, Object, Result, State, Value, ValueKind};
use std::fmt;

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
    /// Returns `true` if this is a [`ResolveKind::Exact`].
    pub fn is_exact(&self) -> bool {
        matches!(self, ResolveKind::Exact(_))
    }

    /// Returns `true` if this is a [`ResolveKind::Any`].
    pub fn is_any(&self) -> bool {
        matches!(self, ResolveKind::Any)
    }

    /// Returns `true` if this is a [`ResolveKind::Maybe`].
    pub fn is_maybe(&self) -> bool {
        matches!(self, ResolveKind::Maybe(_))
    }

    /// Get a collection of [`ValueKind`]s accepted by this [`ResolveKind`].
    pub fn value_kinds(&self) -> Vec<ValueKind> {
        use ResolveKind::*;

        match self {
            Any => ValueKind::all(),
            OneOf(v) => v.clone(),
            Exact(v) => vec![*v],
            Maybe(v) => v.value_kinds(),
        }
    }

    /// Merge two [`ResolveKind`]s, such that the new `ResolveKind` provides the
    /// most constraint possible resolve kind variant.
    pub fn merge(&self, other: &Self) -> Self {
        use ResolveKind::*;

        if let Maybe(kind) = self {
            let other = match other {
                Maybe(v) => v,
                _ => other,
            };

            return Maybe(Box::new(kind.merge(other)));
        }

        if let Maybe(kind) = other {
            return Maybe(Box::new(self.merge(kind)));
        }

        if self.is_any() || other.is_any() {
            return Any;
        }

        let mut kinds: Vec<_> = self
            .value_kinds()
            .into_iter()
            .chain(other.value_kinds().into_iter())
            .collect();

        kinds.sort();
        kinds.dedup();

        if kinds.len() == 1 {
            Exact(kinds[0])
        } else {
            OneOf(kinds)
        }
    }

    /// Returns `true` if the _other_ [`ResolveKind`] is contained within the
    /// current one.
    ///
    /// That is to say, its constraints must be more strict or equal to the
    /// constraints of the current one.
    pub fn contains(&self, other: &Self) -> bool {
        // If we don't expect none, but the other does, the other's requirement
        // is less strict than ours.
        if !self.is_maybe() && other.is_maybe() {
            return false;
        }

        let self_kinds = self.value_kinds();
        let other_kinds = other.value_kinds();

        for kind in other_kinds {
            if !self_kinds.contains(&kind) {
                return false;
            }
        }

        true
    }
}

impl fmt::Display for ResolveKind {
    /// Print a human readable version of the resolve kind constraints.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ResolveKind::*;

        match self {
            Any => f.write_str("any value"),
            OneOf(v) => {
                f.write_str("any of ")?;
                f.write_str(&v.iter().map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
            }
            Exact(v) => f.write_str(&v),
            Maybe(v) => {
                f.write_str("none or ")?;
                f.write_str(&v.to_string())
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains() {
        use ResolveKind::*;
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
        use ResolveKind::*;
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
