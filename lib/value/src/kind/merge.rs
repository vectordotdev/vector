//! All types related to merging one [`Kind`] into another.

use std::{collections::BTreeMap, ops::BitOr};

use super::{Collection, Kind};

impl Kind {
    /// Merge `other` into `self`, using the provided `Strategy`.
    pub fn merge(&mut self, other: Self, strategy: Strategy) {
        self.bytes = self.bytes.or(other.bytes);
        self.integer = self.integer.or(other.integer);
        self.float = self.float.or(other.float);
        self.boolean = self.boolean.or(other.boolean);
        self.timestamp = self.timestamp.or(other.timestamp);
        self.regex = self.regex.or(other.regex);
        self.null = self.null.or(other.null);

        match (self.object.as_mut(), other.object) {
            (None, rhs @ Some(_)) => self.object = rhs,
            (Some(lhs), Some(rhs)) => lhs.merge(rhs, strategy),
            _ => {}
        };

        match (self.array.as_mut(), other.array) {
            (None, rhs @ Some(_)) => self.array = rhs,

            // When the `append` strategy is enabled, we take the higest index of the lhs
            // collection, and increase all known indices of the rhs collection by that value.
            // Then we merge them, similar to the non-append strategy.
            (Some(lhs), Some(rhs)) if strategy.indices.is_append() => {
                let last_index = lhs.known().keys().max().map(|i| *i + 1).unwrap_or_default();

                let (rhs_known, rhs_unknown) = rhs.into_parts();

                let mut known = BTreeMap::default();
                for (index, kind) in rhs_known {
                    let index = index + last_index;
                    known.insert(index, kind);
                }

                lhs.merge(Collection::from_parts(known, rhs_unknown), strategy);
            }

            (Some(lhs), Some(rhs)) => lhs.merge(rhs, strategy),
            _ => {}
        }
    }
}

/// The strategy to apply to the merge between two `Kind`s.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Strategy {
    /// The maximum depth at which to merge nested `Kind`s.
    ///
    /// This can be either "shallow" or "deep", meaning don't merge nested collections, or do merge
    /// them.
    pub depth: Depth,

    /// The strategy used when merging array indices.
    ///
    /// This can either be "keep" or "append", meaning do not update array indices, or append them
    /// to the end of the `self` array.
    pub indices: Indices,
}

/// The choice of "depth" to apply when merging two [`Kind`]s.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Depth {
    /// Do a "shallow" merge for a collection.
    ///
    /// Meaning, only the first-level of elements in the collection are merged. If the collection
    /// contains nested collections, its elements aren't merged, but the collection is swapped out
    /// for the new one.
    Shallow,

    /// Do a "deep" merge for a collection.
    ///
    /// Meaning, collections are recursively merged.
    Deep,
}

impl Depth {
    /// Check if `shallow` strategy is enabled.
    #[must_use]
    pub const fn is_shallow(self) -> bool {
        matches!(self, Self::Shallow)
    }

    /// Check if `deep` strategy is enabled.
    #[must_use]
    pub const fn is_deep(self) -> bool {
        matches!(self, Self::Deep)
    }
}

/// The action to take for arrays and their indices when merging two [`Kind`]s.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Indices {
    /// When merging two arrays, keep their respective indices, potentially merging two `Kinds`
    /// assigned to the same index.
    Keep,

    /// When merging two arrays, append the indices of the `other` array after the indices of the
    /// `self` array.
    ///
    /// Meaning, if `self` has index `0` and `other has index `0`, then the index of `other` is
    /// changed to `1`.
    Append,
}

impl Indices {
    /// Check if `keep` strategy is enabled.
    #[must_use]
    pub const fn is_keep(self) -> bool {
        matches!(self, Self::Keep)
    }

    /// Check if `append` strategy is enabled.
    #[must_use]
    pub const fn is_append(self) -> bool {
        matches!(self, Self::Append)
    }
}

impl BitOr for Kind {
    type Output = Self;

    fn bitor(mut self, rhs: Self) -> Self::Output {
        self.merge(
            rhs,
            Strategy {
                depth: Depth::Deep,
                indices: Indices::Keep,
            },
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::kind::Collection;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_merge() {
        struct TestCase {
            this: Kind,
            other: Kind,
            strategy: Strategy,
            merged: Kind,
        }

        for (
            title,
            TestCase {
                mut this,
                other,
                strategy,
                merged,
            },
        ) in HashMap::from([
            (
                "object field with unknown",
                TestCase {
                    this: Kind::object(Collection::any()),
                    other: Kind::object(BTreeMap::from([("x".into(), Kind::integer())])),
                    strategy: Strategy {
                        depth: Depth::Deep,
                        indices: Indices::Keep,
                    },
                    merged: {
                        let mut collection =
                            Collection::from(BTreeMap::from([("x".into(), Kind::any())]));
                        collection.set_unknown(Kind::any());
                        Kind::object(collection)
                    },
                },
            ),
            (
                "primitives shallow",
                TestCase {
                    this: Kind::bytes(),
                    other: Kind::integer(),
                    strategy: Strategy {
                        depth: Depth::Shallow,
                        indices: Indices::Keep,
                    },
                    merged: Kind::bytes().or_integer(),
                },
            ),
            (
                "primitives deep",
                TestCase {
                    this: Kind::bytes(),
                    other: Kind::integer(),
                    strategy: Strategy {
                        depth: Depth::Deep,

                        indices: Indices::Keep,
                    },
                    merged: Kind::bytes().or_integer(),
                },
            ),
            (
                "mixed unknown shallow",
                TestCase {
                    this: Kind::bytes().or_object(Collection::from_unknown(Kind::integer())),
                    other: Kind::bytes().or_object(Collection::from_unknown(Kind::bytes())),
                    strategy: Strategy {
                        depth: Depth::Shallow,
                        indices: Indices::Keep,
                    },
                    merged: Kind::bytes()
                        .or_object(Collection::from_unknown(Kind::integer().or_bytes())),
                },
            ),
            (
                "mixed unknown deep",
                TestCase {
                    this: Kind::bytes().or_object(Collection::from_unknown(Kind::integer())),
                    other: Kind::bytes().or_object(Collection::from_unknown(Kind::bytes())),
                    strategy: Strategy {
                        depth: Depth::Deep,
                        indices: Indices::Keep,
                    },
                    merged: Kind::bytes()
                        .or_object(Collection::from_unknown(Kind::integer().or_bytes())),
                },
            ),
            (
                "mixed known shallow",
                TestCase {
                    this: Kind::bytes().or_object(BTreeMap::from([
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([
                                ("qux".into(), Kind::bytes()),
                                ("quux".into(), Kind::boolean()),
                                ("this".into(), Kind::timestamp()),
                            ])),
                        ),
                        ("bar".into(), Kind::integer()),
                    ])),
                    other: Kind::integer().or_object(BTreeMap::from([
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([
                                ("qux".into(), Kind::integer()),
                                ("quux".into(), Kind::regex()),
                                ("that".into(), Kind::null()),
                            ])),
                        ),
                        ("baz".into(), Kind::boolean()),
                    ])),
                    strategy: Strategy {
                        depth: Depth::Shallow,
                        indices: Indices::Keep,
                    },
                    merged: Kind::bytes().or_integer().or_object(BTreeMap::from([
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([
                                ("qux".into(), Kind::integer()),
                                ("quux".into(), Kind::regex()),
                                ("that".into(), Kind::null()),
                            ])),
                        ),
                        ("bar".into(), Kind::integer()),
                        ("baz".into(), Kind::boolean()),
                    ])),
                },
            ),
            (
                "mixed known deep",
                TestCase {
                    this: Kind::bytes().or_object(BTreeMap::from([
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([
                                ("qux".into(), Kind::bytes()),
                                ("quux".into(), Kind::boolean()),
                                ("this".into(), Kind::timestamp()),
                            ])),
                        ),
                        ("bar".into(), Kind::integer()),
                    ])),
                    other: Kind::integer().or_object(BTreeMap::from([
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([
                                ("qux".into(), Kind::integer()),
                                ("quux".into(), Kind::regex()),
                                ("that".into(), Kind::null()),
                            ])),
                        ),
                        ("baz".into(), Kind::boolean()),
                    ])),
                    strategy: Strategy {
                        depth: Depth::Deep,
                        indices: Indices::Keep,
                    },
                    merged: Kind::bytes().or_integer().or_object(BTreeMap::from([
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([
                                ("qux".into(), Kind::bytes().or_integer()),
                                ("quux".into(), Kind::boolean().or_regex()),
                                ("this".into(), Kind::timestamp()),
                                ("that".into(), Kind::null()),
                            ])),
                        ),
                        ("bar".into(), Kind::integer()),
                        ("baz".into(), Kind::boolean()),
                    ])),
                },
            ),
            (
                "append array shallow",
                TestCase {
                    this: Kind::array(BTreeMap::from([
                        (0.into(), Kind::bytes()),
                        (
                            2.into(),
                            Kind::array(BTreeMap::from([
                                (0.into(), Kind::integer()),
                                (1.into(), Kind::bytes()),
                            ])),
                        ),
                    ])),
                    other: Kind::array(BTreeMap::from([
                        (0.into(), Kind::regex()),
                        (
                            2.into(),
                            Kind::array(BTreeMap::from([
                                (0.into(), Kind::integer()),
                                (1.into(), Kind::bytes()),
                            ])),
                        ),
                        (5.into(), Kind::timestamp()),
                    ])),
                    strategy: Strategy {
                        depth: Depth::Shallow,
                        indices: Indices::Append,
                    },
                    merged: Kind::array(BTreeMap::from([
                        (0.into(), Kind::bytes()),
                        (
                            2.into(),
                            Kind::array(BTreeMap::from([
                                (0.into(), Kind::integer()),
                                (1.into(), Kind::bytes()),
                            ])),
                        ),
                        (3.into(), Kind::regex()),
                        (
                            5.into(),
                            Kind::array(BTreeMap::from([
                                (0.into(), Kind::integer()),
                                (1.into(), Kind::bytes()),
                            ])),
                        ),
                        (8.into(), Kind::timestamp()),
                    ])),
                },
            ),
            (
                "append array deep",
                TestCase {
                    this: Kind::array(BTreeMap::from([
                        (0.into(), Kind::bytes()),
                        (
                            2.into(),
                            Kind::array(BTreeMap::from([
                                (0.into(), Kind::integer()),
                                (1.into(), Kind::bytes()),
                            ])),
                        ),
                    ])),
                    other: Kind::array(BTreeMap::from([
                        (0.into(), Kind::regex()),
                        (
                            2.into(),
                            Kind::array(BTreeMap::from([
                                (0.into(), Kind::integer()),
                                (1.into(), Kind::bytes()),
                            ])),
                        ),
                        (5.into(), Kind::timestamp()),
                    ])),
                    strategy: Strategy {
                        depth: Depth::Deep,
                        indices: Indices::Append,
                    },
                    merged: Kind::array(BTreeMap::from([
                        (0.into(), Kind::bytes()),
                        (
                            2.into(),
                            Kind::array(BTreeMap::from([
                                (0.into(), Kind::integer()),
                                (1.into(), Kind::bytes()),
                            ])),
                        ),
                        (3.into(), Kind::regex()),
                        (
                            5.into(),
                            Kind::array(BTreeMap::from([
                                (0.into(), Kind::integer()),
                                (1.into(), Kind::bytes()),
                            ])),
                        ),
                        (8.into(), Kind::timestamp()),
                    ])),
                },
            ),
        ]) {
            this.merge(other, strategy);
            assert_eq!(this, merged, "{}", title);
        }
    }
}
