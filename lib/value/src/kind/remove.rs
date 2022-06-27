//! All types related to removing a [`Kind`] nested into another one.

use std::fmt::Display;

use lookup::{Lookup, Segment};

use super::Kind;

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

/// The strategy to apply when removing a `Kind` at a given `Path`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Strategy {
    /// The strategy to apply when the given `Path` contains a "coalesced" segment.
    pub coalesced_path: CoalescedPath,
}

/// The list of errors that can occur when `remove_at_path` fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The error variant triggered by trying to remove the root path.
    RootPath,

    /// The error variant triggered by a negative index in the path.
    NegativeIndexPath,

    /// The error variant triggered by [`CoalescedPath`]'s `Reject` variant.
    CoalescedPath,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootPath => f.write_str("root path removal unsupported"),
            Self::NegativeIndexPath => f.write_str("negative indexing unsupported"),
            Self::CoalescedPath => f.write_str("coalesced path segment rejected"),
        }
    }
}

impl std::error::Error for Error {}

impl Kind {
    /// Remove, and return the `Kind` at the given `path`.
    ///
    /// For arrays, indices are shifted back if any element before the last is removed.
    ///
    /// If the `kind` is a non-collection type, or the path points to a non-existing location in
    /// a collection, this method returns `None`.
    ///
    /// # Errors
    ///
    /// Returns an error when the path contains negative indexing segments (e.g. `.foo[-2]`). This
    /// is currently not supported.
    ///
    /// Returns an error when the path contains a coelesced path segment (e.g. `.(foo | bar)`).
    /// This is currently not supported.
    ///
    /// Returns an error when the path points to its root (`.`). This is because it's ambiguous
    /// whether to return the root-level `object` or `array`, if `Kind` has both defined.
    ///
    /// Use `into_object` or `into_array` if you need the root-level object or array.
    pub fn remove_at_path(
        &mut self,
        path: &Lookup<'_>,
        strategy: Strategy,
    ) -> Result<Option<Self>, Error> {
        // Cannot remove root-path.
        if path.is_root() {
            return Err(Error::RootPath);
        }

        let mut kind = self;
        let mut iter = path.iter().peekable();

        while let Some(segment) = iter.next() {
            let last = iter.peek().is_none();

            kind = match segment {
                // Remove and return the final field.
                Segment::Field(field) if last => {
                    return Ok(kind
                        .object
                        .as_mut()
                        .and_then(|collection| collection.known_mut().remove(&(field.into()))))
                }

                // Try finding the field in the existing object.
                Segment::Field(field) => match kind
                    .object
                    .as_mut()
                    .and_then(|collection| collection.known_mut().get_mut(&(field.into())))
                {
                    Some(kind) => kind,
                    None => return Ok(None),
                },

                // Removal using coalesced path segments is currently unsupported.
                Segment::Coalesce(_) => return Err(Error::CoalescedPath),

                // Remove and return the final matching index. Also down-shift any indices
                // following the removed index.
                Segment::Index(index) if last => {
                    let index = usize::try_from(*index).map_err(|_| Error::NegativeIndexPath)?;

                    let kind = kind.array.as_mut().and_then(|collection| {
                        let kind = collection.known_mut().remove(&(index.into()))?;

                        // Get all indices that we need to down-shift after removing an element.
                        let indices = collection
                            .known()
                            .iter()
                            .filter_map(|(idx, _)| (usize::from(*idx) > index).then(|| idx))
                            .copied()
                            .collect::<Vec<_>>();

                        // Remove all elements for which we need to down-shift the indices.
                        let mut entries = vec![];
                        for index in indices {
                            let kind = collection.known_mut().remove(&index).expect("exists");
                            entries.push((usize::from(index), kind));
                        }

                        // Re-insert all elements with the correct index applied.
                        for (i, kind) in entries {
                            collection.known_mut().insert((i - 1).into(), kind);
                        }

                        Some(kind)
                    });

                    return match kind {
                        Some(kind) => Ok(Some(kind)),
                        None => Ok(None),
                    };
                }

                // Try finding the index in the existing array.
                Segment::Index(index) => match usize::try_from(*index)
                    .map_err(|_| Error::NegativeIndexPath)
                    .map(|index| {
                        kind.array
                            .as_mut()
                            .and_then(|collection| collection.known_mut().get_mut(&(index.into())))
                    })? {
                    Some(kind) => kind,
                    None => return Ok(None),
                },
            };
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use lookup::LookupBuf;

    use super::*;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_remove_at_path() {
        struct TestCase {
            kind: Kind,
            path: LookupBuf,
            strategy: Strategy,
            returned: Result<Option<Kind>, Error>,
            mutated: Kind,
        }

        for (
            title,
            TestCase {
                mut kind,
                path,
                strategy,
                returned,
                mutated,
            },
        ) in HashMap::from([
            (
                "primitive",
                TestCase {
                    kind: Kind::bytes(),
                    path: "foo".into(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    returned: Ok(None),
                    mutated: Kind::bytes(),
                },
            ),
            (
                "multiple primitives",
                TestCase {
                    kind: Kind::integer().or_regex(),
                    path: "foo".into(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    returned: Ok(None),
                    mutated: Kind::integer().or_regex(),
                },
            ),
            (
                "object w/ matching path",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                    path: "foo".into(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    returned: Ok(Some(Kind::integer())),
                    mutated: Kind::object(BTreeMap::default()),
                },
            ),
            (
                "object w/o matching path",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                    path: "bar".into(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    returned: Ok(None),
                    mutated: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                },
            ),
            (
                "array w/ matching path",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[1]").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    returned: Ok(Some(Kind::integer())),
                    mutated: Kind::array(BTreeMap::default()),
                },
            ),
            (
                "array w/o matching path",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    returned: Ok(None),
                    mutated: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                },
            ),
            (
                "array w/ negative indexing",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[-1]").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    returned: Err(Error::NegativeIndexPath),
                    mutated: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                },
            ),
            (
                "array w/ matching path, shifting indices",
                TestCase {
                    kind: Kind::array(BTreeMap::from([
                        (1.into(), Kind::integer()),
                        (2.into(), Kind::bytes()),
                        (3.into(), Kind::boolean()),
                        (4.into(), Kind::regex()),
                    ])),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    returned: Ok(Some(Kind::bytes())),
                    mutated: Kind::array(BTreeMap::from([
                        (1.into(), Kind::integer()),
                        (2.into(), Kind::boolean()),
                        (3.into(), Kind::regex()),
                    ])),
                },
            ),
            (
                "complex pathing",
                TestCase {
                    kind: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([
                            (1.into(), Kind::integer()),
                            (
                                2.into(),
                                Kind::object(BTreeMap::from([
                                    (
                                        "bar".into(),
                                        Kind::object(BTreeMap::from([(
                                            "baz".into(),
                                            Kind::integer().or_regex(),
                                        )])),
                                    ),
                                    ("qux".into(), Kind::boolean()),
                                ])),
                            ),
                        ])),
                    )])),
                    path: LookupBuf::from_str(".foo[2].bar").unwrap(),
                    strategy: Strategy {
                        coalesced_path: CoalescedPath::Reject,
                    },
                    returned: Ok(Some(Kind::object(BTreeMap::from([(
                        "baz".into(),
                        Kind::integer().or_regex(),
                    )])))),
                    mutated: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([
                            (1.into(), Kind::integer()),
                            (
                                2.into(),
                                Kind::object(BTreeMap::from([("qux".into(), Kind::boolean())])),
                            ),
                        ])),
                    )])),
                },
            ),
        ]) {
            let got = kind.remove_at_path(&path.to_lookup(), strategy);

            assert_eq!(got, returned, "returned: {}", title);
            assert_eq!(kind, mutated, " mutated: {}", title);
        }
    }
}
