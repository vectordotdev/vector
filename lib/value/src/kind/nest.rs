//! All types related to nesting a [`Kind`] into a path.

use std::collections::BTreeMap;

use std::fmt::Display;

use lookup::{Lookup, Segment};

use super::{Collection, Kind};

/// The strategy to use when a given path contains a coalesced segment.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CoalescedPath {
    /// Reject coalesced path segments during removal, returning an error.
    Reject,
}

impl CoalescedPath {
    /// Check if the active strategy is "reject".
    #[must_use]
    pub const fn is_reject(&self) -> bool {
        matches!(self, Self::Reject)
    }
}

/// The strategy to apply when nesting a `Kind` at a given `Path`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Strategy {
    /// The strategy to apply when the given `Path` contains a "coalesced" segment.
    pub coalesced_path: CoalescedPath,
}

/// The list of errors that can occur when `remove_at_path` fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The error variant triggered by a negative index in the path.
    NegativeIndexPath,

    /// The error variant triggered by [`CoalescedPath`]'s `Reject` variant.
    CoalescedPath,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NegativeIndexPath => f.write_str("negative indexing unsupported"),
            Error::CoalescedPath => f.write_str("coalesced path segment rejected"),
        }
    }
}

impl std::error::Error for Error {}

impl Kind {
    /// Nest the given [`Kind`] into a provided path.
    ///
    /// For example, given an `integer` kind and a path `.foo`, a new `Kind` is returned that is
    /// known to be an object, of which the `foo` field is known to be an `integer`.
    ///
    /// # Errors
    ///
    /// Returns an error when the path contains negative indexing segments (e.g. `.foo[-2]`). This
    /// is currently not supported.
    ///
    /// Returns an error when the path contains a coelesced path segment (e.g. `.(foo | bar)`).
    /// This is currently not supported.
    pub fn nest_at_path(mut self, path: &Lookup<'_>, strategy: Strategy) -> Result<Self, Error> {
        fn object_from_field(field: &lookup::Field<'_>, kind: Kind) -> Kind {
            let map = BTreeMap::from([(field.into(), kind)]);
            Kind::object(map)
        }

        for segment in path.iter().rev() {
            match segment {
                Segment::Field(field) => {
                    self = object_from_field(field, self);
                }
                Segment::Coalesce(fields) => return Err(Error::CoalescedPath),
                Segment::Index(index) => {
                    self = Self::array(
                        usize::try_from(*index)
                            .map_err(|_| Error::NegativeIndexPath)
                            .map(|index| {
                                let map = BTreeMap::from([(index.into(), self)]);
                                Collection::from(map)
                            })?,
                    );
                }
            }
        }

        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use lookup::LookupBuf;

    use super::*;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_nest_at_path() {
        struct TestCase {
            kind: Kind,
            path: LookupBuf,
            strategy: Strategy,
            want: Result<Kind, Error>,
        }

        for (
            title,
            TestCase {
                kind,
                path,
                strategy,
                want,
            },
        ) in HashMap::from([
            (
                "single-level object",
                TestCase {
                    kind: Kind::bytes(),
                    path: "foo".into(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    want: Ok(Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::bytes(),
                    )]))),
                },
            ),
            (
                "multi-level object",
                TestCase {
                    kind: Kind::boolean(),
                    path: LookupBuf::from_str("foo.bar").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    want: Ok(Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([("bar".into(), Kind::boolean())])),
                    )]))),
                },
            ),
            (
                "array positive index",
                TestCase {
                    kind: Kind::integer(),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    want: Ok(Kind::array(BTreeMap::from([(2.into(), Kind::integer())]))),
                },
            ),
            (
                "array negative index",
                TestCase {
                    kind: Kind::integer(),
                    path: LookupBuf::from_str("[-2]").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    want: Err(Error::NegativeIndexPath),
                },
            ),
            (
                "coalesced path",
                TestCase {
                    kind: Kind::integer(),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    want: Err(Error::CoalescedPath),
                },
            ),
            (
                "mixed path",
                TestCase {
                    kind: Kind::integer().or_bytes(),
                    path: LookupBuf::from_str(".foo.bar[1].baz").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    want: Ok(Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([(
                            "bar".into(),
                            Kind::array(BTreeMap::from([(
                                1.into(),
                                Kind::object(BTreeMap::from([(
                                    "baz".into(),
                                    Kind::integer().or_bytes(),
                                )])),
                            )])),
                        )])),
                    )]))),
                },
            ),
        ]) {
            assert_eq!(
                kind.nest_at_path(&path.to_lookup(), strategy),
                want,
                "{}",
                title
            );
        }
    }
}
