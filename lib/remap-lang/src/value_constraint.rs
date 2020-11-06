use crate::ValueKind;
use std::fmt;

/// The constraint of a set of [`ValueKind`]s.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueConstraint {
    /// Any value kind is accepted.
    Any,

    /// A subset of value kinds is accepted.
    OneOf(Vec<ValueKind>),

    /// Exactly one value kind is accepted
    Exact(ValueKind),

    /// Either none, or a value kind constraint is accepted.
    ///
    /// For example, and if-statement without an else-condition can resolve to
    /// nothing if the if-condition does not match.
    Maybe(Box<ValueConstraint>),
}

impl ValueConstraint {
    /// Returns `true` if this is a [`ValueConstraint::Exact`].
    pub fn is_exact(&self) -> bool {
        matches!(self, Self::Exact(_))
    }

    /// Returns `true` if this is a [`ValueConstraint::Any`].
    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Returns `true` if this is a [`ValueConstraint::Maybe`].
    pub fn is_maybe(&self) -> bool {
        matches!(self, Self::Maybe(_))
    }

    /// Get a collection of [`ValueKind`]s accepted by this [`ValueConstraint`].
    pub fn value_kinds(&self) -> Vec<ValueKind> {
        use ValueConstraint::*;

        match self {
            Any => ValueKind::all(),
            OneOf(v) => v.clone(),
            Exact(v) => vec![*v],
            Maybe(v) => v.value_kinds(),
        }
    }

    /// Merge two [`ValueConstraint`]s, such that the new `ValueConstraint`
    /// provides the most constraint possible value constraint.
    pub fn merge(&self, other: &Self) -> Self {
        use ValueConstraint::*;

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

    /// Returns `true` if the _other_ [`ValueConstraint`] is contained within
    /// the current one.
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

impl fmt::Display for ValueConstraint {
    /// Print a human readable version of the value constraint.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ValueConstraint::*;

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
