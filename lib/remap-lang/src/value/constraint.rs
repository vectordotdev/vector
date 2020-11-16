use crate::value;
use std::fmt;

/// The constraint of a set of [`value::Kind`]s.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Constraint {
    /// Any value kind is accepted.
    Any,

    /// Exactly one value kind is accepted
    Exact(value::Kind),

    /// A subset of value kinds is accepted.
    OneOf(Vec<value::Kind>),
}

impl Default for Constraint {
    fn default() -> Self {
        Constraint::Any
    }
}

impl<T: Into<value::Kind>> From<T> for Constraint {
    fn from(kind: T) -> Self {
        Constraint::Exact(kind.into())
    }
}

impl From<Vec<value::Kind>> for Constraint {
    fn from(kinds: Vec<value::Kind>) -> Self {
        debug_assert!(kinds.len() > 1);

        Constraint::OneOf(kinds)
    }
}

impl Constraint {
    /// Returns `true` if this is a [`Constraint::Exact`].
    pub fn is_exact(&self) -> bool {
        matches!(self, Self::Exact(_))
    }

    /// Returns `true` if this is a [`Constraint::Any`].
    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Returns `true` if this constraint exactly matches `other`.
    pub fn is(&self, other: impl Into<Self>) -> bool {
        self == &other.into()
    }

    /// Get a collection of [`value::Kind`]s accepted by this [`Constraint`].
    pub fn value_kinds(&self) -> Vec<value::Kind> {
        use Constraint::*;

        match self {
            Any => value::Kind::all(),
            OneOf(v) => v.clone(),
            Exact(v) => vec![*v],
        }
    }

    /// Merge two [`Constraint`]s, such that the new `Constraint` provides the
    /// most constraint possible value constraint.
    pub fn merge(&self, other: &Self) -> Self {
        use Constraint::*;

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

    /// Returns `true` if the _other_ [`Constraint`] is contained within the
    /// current one.
    ///
    /// That is to say, its constraints must be more strict or equal to the
    /// constraints of the current one.
    pub fn contains(&self, other: &Self) -> bool {
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

impl fmt::Display for Constraint {
    /// Print a human readable version of the value constraint.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Constraint::*;

        match self {
            Any => f.write_str("any"),
            OneOf(v) => {
                let mut kinds = v.iter().map(|v| v.as_str()).collect::<Vec<_>>();

                let last = kinds.pop();
                let mut string = kinds.join(", ");

                if let Some(last) = last {
                    if !string.is_empty() {
                        string.push_str(" or ")
                    }

                    string.push_str(last);
                }

                f.write_str(&string)
            }
            Exact(v) => f.write_str(&v),
        }
    }
}
