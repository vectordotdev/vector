use crate::ValueKind;
use std::fmt;

/// The constraint of a set of [`ValueKind`]s.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueConstraint {
    /// Any value kind is accepted.
    Any,

    /// Exactly one value kind is accepted
    Exact(ValueKind),

    /// A subset of value kinds is accepted.
    OneOf(Vec<ValueKind>),
}

impl Default for ValueConstraint {
    fn default() -> Self {
        ValueConstraint::Any
    }
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

    /// Get a collection of [`ValueKind`]s accepted by this [`ValueConstraint`].
    pub fn value_kinds(&self) -> Vec<ValueKind> {
        use ValueConstraint::*;

        match self {
            Any => ValueKind::all(),
            OneOf(v) => v.clone(),
            Exact(v) => vec![*v],
        }
    }

    /// Merge two [`ValueConstraint`]s, such that the new `ValueConstraint`
    /// provides the most constraint possible value constraint.
    pub fn merge(&self, other: &Self) -> Self {
        use ValueConstraint::*;

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
