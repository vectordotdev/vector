//! All types related to merging one [`Kind`] into another.

use crate::kind::Field;
use std::{collections::BTreeMap, ops::BitOr};

use super::{Collection, Kind};

impl Kind {
    /// Merge `other` into `self`, using the provided `Strategy`.
    pub fn merge(&mut self, other: Self, strategy: Strategy) {
        match strategy.indices {
            Indices::Keep => self.merge_keep(other, strategy.collisions.is_shallow()),
            Indices::Append => self.merge_append(other, strategy.collisions.is_shallow()),
        }
    }

    fn merge_primitives(&mut self, other: &Self) {
        self.bytes = self.bytes.or(other.bytes);
        self.integer = self.integer.or(other.integer);
        self.float = self.float.or(other.float);
        self.boolean = self.boolean.or(other.boolean);
        self.timestamp = self.timestamp.or(other.timestamp);
        self.regex = self.regex.or(other.regex);
        self.null = self.null.or(other.null);
    }

    fn merge_objects(&mut self, other: Option<Collection<Field>>, overwrite: bool) {
        match (self.object.as_mut(), other) {
            (None, rhs @ Some(_)) => self.object = rhs,
            (Some(lhs), Some(rhs)) => lhs.merge(rhs, overwrite),
            _ => {}
        };
    }

    /// Merge `other` into `self`, optionally overwriting on conflicts.
    pub fn merge_keep(&mut self, other: Self, overwrite: bool) {
        self.merge_primitives(&other);
        self.merge_objects(other.object, overwrite);

        match (self.array.as_mut(), other.array) {
            (None, Some(rhs)) => self.array = Some(rhs),
            (Some(lhs), Some(rhs)) => lhs.merge(rhs, overwrite),
            _ => {}
        }
    }

    /// Merge `other` into `self`, using the provided `Strategy`.
    /// We take the higest index of the lhs
    /// collection, and increase all known indices of the rhs collection by that value.
    /// Then we merge them, similar to the non-append strategy.
    fn merge_append(&mut self, other: Self, overwrite: bool) {
        self.merge_primitives(&other);
        self.merge_objects(other.object, overwrite);

        match (self.array.as_mut(), other.array) {
            (None, Some(rhs)) => self.array = Some(rhs),

            (Some(lhs), Some(rhs)) => {
                let last_index = lhs.known().keys().max().map(|i| *i + 1).unwrap_or_default();

                let (rhs_known, rhs_unknown) = rhs.into_parts();

                let mut known = BTreeMap::default();
                for (index, kind) in rhs_known {
                    let index = index + last_index;
                    known.insert(index, kind);
                }

                // The indices cannot collide since they are being appended, so switch to overwrite mode.
                // otherwise the union of the types will include "null"
                lhs.merge(Collection::from_parts(known, rhs_unknown), true);
            }
            _ => {}
        }
    }
}

/// The strategy to apply to the merge between two `Kind`s.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Strategy {
    /// How to deal with types in a collection if both specify a type.
    /// This only applies to types in a collection (not the root type)
    pub collisions: CollisionStrategy,

    /// The strategy used when merging array indices.
    ///
    /// This can either be "keep" or "append", meaning do not update array indices, or append them
    /// to the end of the `self` array.
    pub indices: Indices,
}

/// The choice of "depth" to apply when merging two [`Kind`]s.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CollisionStrategy {
    /// Use the 2nd value
    Overwrite,

    /// Merge both together
    Union,
}

impl CollisionStrategy {
    /// Check if `shallow` strategy is enabled.
    #[must_use]
    pub const fn is_shallow(self) -> bool {
        matches!(self, Self::Overwrite)
    }

    /// Check if `deep` strategy is enabled.
    #[must_use]
    pub const fn is_deep(self) -> bool {
        matches!(self, Self::Union)
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
                collisions: CollisionStrategy::Union,
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
                        collisions: CollisionStrategy::Union,
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
                        collisions: CollisionStrategy::Overwrite,
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
                        collisions: CollisionStrategy::Union,

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
                        collisions: CollisionStrategy::Overwrite,
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
                        collisions: CollisionStrategy::Union,
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
                        collisions: CollisionStrategy::Overwrite,
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
                        collisions: CollisionStrategy::Union,
                        indices: Indices::Keep,
                    },
                    merged: Kind::bytes().or_integer().or_object(BTreeMap::from([
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([
                                ("qux".into(), Kind::bytes().or_integer()),
                                ("quux".into(), Kind::boolean().or_regex()),
                                ("this".into(), Kind::timestamp().or_null()),
                                ("that".into(), Kind::null()),
                            ])),
                        ),
                        ("bar".into(), Kind::integer().or_null()),
                        ("baz".into(), Kind::boolean().or_null()),
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
                        collisions: CollisionStrategy::Overwrite,
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
                        collisions: CollisionStrategy::Union,
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
