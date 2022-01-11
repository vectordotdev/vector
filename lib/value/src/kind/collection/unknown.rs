use std::ops::{BitAnd, BitOr, BitXor};

use super::Exact;
use crate::Kind;

/// The type-state of "unknown" values in a collection.
///
/// That is, given a collection, it can have a set of "known" value types (e.g. we know the object
/// collection has a field `.foo` with a type `integer`), but also a singular "unknown" value type
/// (e.g. the array collection has an integer value at index 0, and is 3 values in size. We don't
/// know the exact values for indices 1 and 2, but we do know that it has to be the type defined by
/// `Unknown`).
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Unknown {
    Any,
    Exact(Exact),
}

impl Unknown {
    /// Get the `any` state for `Unknown`.
    #[must_use]
    pub fn any() -> Self {
        Self::Any
    }

    /// Get the `exact` state for `Unknown`.
    #[must_use]
    pub fn exact(exact: Exact) -> Self {
        Self::Exact(exact)
    }

    /// Get the `json` state for `Unknown`.
    #[must_use]
    pub fn json() -> Self {
        Self::exact(Exact::json())
    }

    /// Check if the state of `Unknown` is "any".
    #[must_use]
    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Check if the state of `Unknown` is "exact".
    #[must_use]
    pub fn is_exact(&self) -> bool {
        matches!(self, Self::Exact(_))
    }

    /// Check if `self` is a superset of `other`.
    ///
    /// Meaning, if `self` is `Any`, then it's always a superset of `other`, otherwise its
    /// accumulative types need to be a superset of `other`.
    pub fn is_superset(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Any, _) => true,
            (Self::Exact(lhs), Unknown::Exact(rhs)) => lhs.is_superset(rhs),

            // Technically, `Exact` can have all states set to `true`, which would be the same as
            // `Unknown::Any`, but this is an invalid invariant, and considered a programming bug.
            (Self::Exact(_), Self::Any) => false,
        }
    }
}

impl BitOr for Unknown {
    type Output = Self;

    /// If the state of `Unknown` fields is `any`, this method always returns `any`.
    ///
    /// Otherwise, a bit-or operation is performed on the `Exact` state.
    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Any, _) | (_, Self::Any) => Self::Any,
            (Self::Exact(lhs), Self::Exact(rhs)) => Self::Exact(lhs | rhs),
        }
    }
}

impl BitXor for Unknown {
    type Output = Self;

    /// If the state of `Unknown` fields is `any`, this method always returns `any`.
    ///
    /// Otherwise, a bit-xor operation is performed on the `Exact` state.
    fn bitxor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Any, _) | (_, Self::Any) => Self::Any,
            (Self::Exact(lhs), Self::Exact(rhs)) => Self::Exact(lhs ^ rhs),
        }
    }
}

impl BitAnd for Unknown {
    type Output = Self;

    /// If the state of `Unknown` fields is `any`, this method always returns `any`.
    ///
    /// Otherwise, a bit-and operation is performed on the `Exact` state.
    fn bitand(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Any, _) | (_, Self::Any) => Self::Any,
            (Self::Exact(lhs), Self::Exact(rhs)) => Self::Exact(lhs & rhs),
        }
    }
}

impl From<Unknown> for Kind {
    fn from(unknown: Unknown) -> Self {
        match unknown {
            Unknown::Any => Kind::any(),
            Unknown::Exact(exact) => exact.into(),
        }
    }
}

impl From<Kind> for Unknown {
    fn from(kind: Kind) -> Self {
        (&kind).into()
    }
}

impl From<&Kind> for Unknown {
    fn from(kind: &Kind) -> Self {
        if kind.is_any() {
            return Unknown::any();
        }

        Unknown::Exact(kind.into())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_is_superset() {
        struct TestCase {
            this: Unknown,
            other: Unknown,
            want: bool,
        }

        for (title, TestCase { this, other, want }) in HashMap::from([
            (
                "any comparison",
                TestCase {
                    this: Unknown::any(),
                    other: Unknown::any(),
                    want: true,
                },
            ),
            (
                "exact/any mismatch",
                TestCase {
                    this: Unknown::Exact(Exact::json()),
                    other: Unknown::any(),
                    want: false,
                },
            ),
            (
                "any/exact match",
                TestCase {
                    this: Unknown::any(),
                    other: Unknown::Exact(Exact::json()),
                    want: true,
                },
            ),
            (
                "exact matching comparison",
                TestCase {
                    this: Unknown::Exact(Exact::json()),
                    other: Unknown::Exact(Exact::json()),
                    want: true,
                },
            ),
            (
                "exact mismatch comparison",
                TestCase {
                    this: Unknown::Exact(Exact {
                        bytes: true,
                        integer: false,
                        float: false,
                        boolean: false,
                        timestamp: false,
                        regex: false,
                        null: false,
                        object: false,
                        array: false,
                    }),
                    other: Unknown::Exact(Exact {
                        bytes: false,
                        integer: true,
                        float: false,
                        boolean: false,
                        timestamp: false,
                        regex: false,
                        null: false,
                        object: false,
                        array: false,
                    }),
                    want: false,
                },
            ),
        ]) {
            assert_eq!(this.is_superset(&other), want, "{}", title);
        }
    }
}
