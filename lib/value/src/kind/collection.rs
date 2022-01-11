mod exact;
mod field;
mod index;
mod unknown;

use std::collections::BTreeMap;

use super::{merge, Kind};
use exact::Exact;
pub use field::Field;
pub use index::Index;
use unknown::Unknown;

/// The kinds of a collection (e.g. array or object).
///
/// A collection contains one or more kinds for known positions within the collection (e.g. indices
/// or fields), and contains a global "unknown" state that applies to all unknown paths.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Collection<T: Ord> {
    known: BTreeMap<T, Kind>,

    /// The kind of other unknown fields.
    ///
    /// For example, an array collection might be known to have an "integer" state at the 0th
    /// index, but it has an unknown length. It is however known that whatever length the array
    /// has, its values can only be integers or floats, so the `unknown` state is set to those two.
    unknown: Unknown,
}

impl<T: Ord> Collection<T> {
    /// Create a new collection from its parts.
    #[must_use]
    pub(super) fn from_parts(known: BTreeMap<T, Kind>, unknown: impl Into<Unknown>) -> Self {
        Self {
            known,
            unknown: unknown.into(),
        }
    }

    /// Create a new collection with a defined "unknown fields" value, and no known fields.
    pub fn unknown(unknown: impl Into<Unknown>) -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: unknown.into(),
        }
    }

    /// Create a collection kind of which the encapsulated values can be any kind.
    #[must_use]
    pub fn any() -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: Unknown::any(),
        }
    }

    /// Create a collection kind of which the encapsulated values can be any JSON-compatible kind.
    #[must_use]
    pub fn json() -> Self {
        Self {
            known: BTreeMap::default(),
            unknown: Unknown::json(),
        }
    }

    /// Check if the collection fields can be of any kind.
    ///
    /// This returns `false` if at least _one_ field kind is known.
    #[must_use]
    pub fn is_any(&self) -> bool {
        self.known.iter().all(|(_, k)| k.is_any()) && self.unknown.is_any()
    }

    /// Get the "known" and "unknown" parts of the collection.
    #[must_use]
    pub(super) fn into_parts(self) -> (BTreeMap<T, Kind>, Unknown) {
        (self.known, self.unknown)
    }

    /// Get the "known" field value kinds.
    #[must_use]
    pub fn known(&self) -> &BTreeMap<T, Kind> {
        &self.known
    }

    /// Get a mutable reference to "known" field value kinds.
    pub fn known_mut(&mut self) -> &mut BTreeMap<T, Kind> {
        &mut self.known
    }

    /// Get the "unknown" field value kind.
    #[must_use]
    pub fn as_unknown(&self) -> &Unknown {
        &self.unknown
    }

    /// Set the "unknown" field values to the given kind.
    pub fn set_unknown(&mut self, unknown: impl Into<Unknown>) {
        self.unknown = unknown.into();
    }

    /// Check if `self` is a superset of `other`.
    ///
    /// Meaning, for all known fields in `other`, if the field also exists in `self`, then its type
    /// needs to be a subset of `self`, otherwise its type needs to be a subset of self's
    /// `unknown`.
    ///
    /// If `self` has known fields not defined in `other`, then `other`'s `unknown` must be
    /// a superset of those fields defined in `self`.
    ///
    /// Additionally, other's `unknown` type needs to be a subset of `self`'s.
    #[must_use]
    pub fn is_superset(&self, other: &Self) -> bool {
        // `self`'s `unknown` needs to be  a superset of `other`'s.
        if !self.unknown.is_superset(&other.unknown) {
            return false;
        }

        // All known fields in `other` need to either be a subset of a matching known field in
        // `self`, or a subset of self's `unknown` type state.
        if !other
            .known
            .iter()
            .all(|(key, other_kind)| match self.known.get(key) {
                Some(self_kind) => self_kind.is_superset(other_kind),
                None => Kind::from(self.unknown).is_superset(other_kind),
            })
        {
            return false;
        }

        // All known fields in `self` not known in `other` need to be a superset of other's
        // `unknown` type state.
        self.known
            .iter()
            .all(|(key, self_kind)| match other.known.get(key) {
                Some(_) => true,
                None => self_kind.is_superset(&other.unknown.into()),
            })
    }

    /// Merge the `other` collection into `self`.
    ///
    /// The following merge strategies are applied.
    ///
    /// For *known fields*:
    ///
    /// - If a field exists in both collections, their `Kind`s are merged, or the `other` fields
    ///   are used (depending on the configured [`Strategy`](merge::Strategy).
    ///
    /// - If a field exists in one but not the other, the field is used.
    ///
    /// For *unknown fields or indices*:
    ///
    /// - Both `Unknown`s are merged, similar to merging two `Kind`s.
    pub fn merge(&mut self, mut other: Self, strategy: merge::Strategy) {
        self.known
            .iter_mut()
            .for_each(|(key, self_kind)| match other.known.remove(&key) {
                Some(other_kind) if strategy.depth.is_shallow() => *self_kind = other_kind,
                Some(other_kind) => self_kind.merge(other_kind, strategy),
                _ => {}
            });

        self.known.extend(other.known.into_iter());
        self.unknown = self.unknown | other.unknown;
    }
}

impl<T: Ord> From<BTreeMap<T, Kind>> for Collection<T> {
    fn from(known: BTreeMap<T, Kind>) -> Self {
        Self {
            known,
            unknown: Unknown::any(),
        }
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
            this: Collection<&'static str>,
            other: Collection<&'static str>,
            want: bool,
        }

        for (title, TestCase { this, other, want }) in HashMap::from([
            (
                "any comparison",
                TestCase {
                    this: Collection::any(),
                    other: Collection::any(),
                    want: true,
                },
            ),
            (
                "exact/any mismatch",
                TestCase {
                    this: Collection::json(),
                    other: Collection::any(),
                    want: false,
                },
            ),
            (
                "unknown match",
                TestCase {
                    this: Collection::unknown(Kind::regex().or_null()),
                    other: Collection::unknown(Kind::regex()),
                    want: true,
                },
            ),
            (
                "unknown mis-match",
                TestCase {
                    this: Collection::unknown(Kind::regex().or_null()),
                    other: Collection::unknown(Kind::bytes()),
                    want: false,
                },
            ),
            (
                "other-known match",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([("bar", Kind::bytes())]),
                        Kind::regex().or_null(),
                    ),
                    other: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::regex()), ("bar", Kind::bytes())]),
                        Kind::regex(),
                    ),
                    want: true,
                },
            ),
            (
                "other-known mis-match",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::integer()), ("bar", Kind::bytes())]),
                        Kind::regex().or_null(),
                    ),
                    other: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::regex()), ("bar", Kind::bytes())]),
                        Kind::regex(),
                    ),
                    want: false,
                },
            ),
            (
                "self-known match",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([
                            ("foo", Kind::bytes().or_integer()),
                            ("bar", Kind::bytes().or_integer()),
                        ]),
                        Kind::bytes().or_integer(),
                    ),
                    other: Collection::unknown(Kind::bytes().or_integer()),
                    want: true,
                },
            ),
            (
                "self-known mis-match",
                TestCase {
                    this: Collection::from_parts(
                        BTreeMap::from([("foo", Kind::integer()), ("bar", Kind::bytes())]),
                        Kind::bytes().or_integer(),
                    ),
                    other: Collection::unknown(Kind::bytes().or_integer()),
                    want: false,
                },
            ),
        ]) {
            assert_eq!(this.is_superset(&other), want, "{}", title);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_merge() {
        struct TestCase {
            this: Collection<&'static str>,
            other: Collection<&'static str>,
            strategy: merge::Strategy,
            want: Collection<&'static str>,
        }

        for (
            title,
            TestCase {
                mut this,
                other,
                strategy,
                want,
            },
        ) in HashMap::from([
            (
                "any merge",
                TestCase {
                    this: Collection::any(),
                    other: Collection::any(),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::any(),
                },
            ),
            (
                "json merge",
                TestCase {
                    this: Collection::json(),
                    other: Collection::json(),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::json(),
                },
            ),
            (
                "any w/ json merge",
                TestCase {
                    this: Collection::any(),
                    other: Collection::json(),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::any(),
                },
            ),
            (
                "merge same knowns",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("foo", Kind::bytes())])),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::from(BTreeMap::from([("foo", Kind::integer().or_bytes())])),
                },
            ),
            (
                "append different knowns",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([("bar", Kind::bytes())])),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::from(BTreeMap::from([
                        ("foo", Kind::integer()),
                        ("bar", Kind::bytes()),
                    ])),
                },
            ),
            (
                "merge/append same/different knowns",
                TestCase {
                    this: Collection::from(BTreeMap::from([("foo", Kind::integer())])),
                    other: Collection::from(BTreeMap::from([
                        ("foo", Kind::bytes()),
                        ("bar", Kind::boolean()),
                    ])),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::from(BTreeMap::from([
                        ("foo", Kind::integer().or_bytes()),
                        ("bar", Kind::boolean()),
                    ])),
                },
            ),
            (
                "merge unknowns",
                TestCase {
                    this: Collection::unknown(Kind::bytes()),
                    other: Collection::unknown(Kind::integer()),
                    strategy: merge::Strategy {
                        depth: merge::Depth::Deep,
                        indices: merge::Indices::Keep,
                    },
                    want: Collection::unknown(Kind::bytes().or_integer()),
                },
            ),
        ]) {
            this.merge(other, strategy);

            assert_eq!(this, want, "{}", title);
        }
    }
}
