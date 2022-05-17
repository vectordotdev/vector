//! All types related to finding a [`Kind`] nested into another one.

use std::{borrow::Cow, collections::VecDeque};

use lookup::{Field, Lookup, Segment};

use super::Kind;
use crate::kind::{merge, EmptyKindError};

/// The list of errors that can occur when `remove_at_path` fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The error variant triggered by a negative index in the path.
    NegativeIndexPath,
}

impl Kind {
    /// Find the [`Kind`] at the given path.
    ///
    /// If the path points to root, then `self` is returned, otherwise `None` is returned if `Kind`
    /// isn't an object or array. If the path points to a non-existing element in an existing collection,
    /// then the collection's `unknown` `Kind` variant is returned.
    ///
    /// # Errors
    ///
    /// Returns an error when the path contains negative indexing segments (e.g. `.foo[-2]`). This
    /// is currently not supported.
    #[allow(clippy::too_many_lines)]
    pub fn find_at_path<'a>(
        &'a self,
        path: &'a Lookup<'a>,
    ) -> Result<Option<Cow<'a, Self>>, Error> {
        enum InnerKind<'a> {
            Exact(&'a Kind),
            Infinite(Kind),
        }

        use Cow::{Borrowed, Owned};

        // This recursively tries to get the field within a `Kind`'s object.
        //
        // It returns `None` if:
        //
        // - The provided `Kind` isn't an object.
        // - The `Kind`'s object does not contain a known field matching `field` *and* its unknown
        // fields either aren't an object, or they (recursively) don't match these two rules.
        fn get_field_from_object<'a>(
            kind: &'a Kind,
            field: &'a Field<'a>,
        ) -> Option<InnerKind<'a>> {
            kind.object.as_ref().and_then(|collection| {
                collection
                    .known()
                    .get(&(field.into()))
                    .map(InnerKind::Exact)
                    .or_else(|| {
                        collection.unknown().as_ref().and_then(|unknown| {
                            unknown.as_exact().map(InnerKind::Exact).or_else(|| {
                                Some(InnerKind::Infinite(unknown.to_kind().into_owned()))
                            })
                        })
                    })
            })
        }

        // This recursively tries to get the index within a `Kind`'s array.
        //
        // It returns `None` if:
        //
        // - The provided `Kind` isn't an array.
        // - The `Kind`'s array does not contain a known index matching `index` *and* its unknown
        // indices either aren't an array, or they (recursively) don't match these two rules.
        fn get_element_from_array(kind: &Kind, index: usize) -> Option<InnerKind<'_>> {
            kind.array.as_ref().and_then(|collection| {
                collection
                    .known()
                    .get(&(index.into()))
                    .map(InnerKind::Exact)
                    .or_else(|| {
                        collection.unknown().as_ref().and_then(|unknown| {
                            unknown.as_exact().map(InnerKind::Exact).or_else(|| {
                                Some(InnerKind::Infinite(unknown.to_kind().into_owned()))
                            })
                        })
                    })
            })
        }

        if path.is_root() {
            return Ok(Some(Borrowed(self)));
        }

        // While iterating through the path segments, one or more segments might point to a `Kind`
        // that has more than one state defined. In such a case, there is no way of knowing whether
        // we're going to see the expected collection state at runtime, so we need to take into
        // account the fact that the traversal might not succeed, and thus return `null`.
        let mut or_null = false;

        let mut kind = self;
        let mut iter = path.iter().peekable();

        while let Some(segment) = iter.next() {
            if !kind.is_exact() {
                or_null = true;
            }

            kind = match segment {
                // Try finding the field in the existing object.
                Segment::Field(field) => match get_field_from_object(kind, field) {
                    None => return Ok(None),

                    Some(InnerKind::Exact(kind)) => kind,

                    // We're dealing with an infinite recursive type, so there's no need to
                    // further expand on the path.
                    Some(InnerKind::Infinite(kind)) => {
                        return Ok(Some(Owned(if or_null { kind.or_null() } else { kind })))
                    }
                },

                Segment::Coalesce(fields) => {
                    let mut merged_kind = Self::empty();

                    for field in fields {
                        let mut segments = iter.clone().cloned().collect::<VecDeque<_>>();
                        segments.push_front(Segment::Field(field.clone()));
                        let path = Lookup::from(segments);

                        match kind.find_at_path(&path)? {
                            None => {
                                merged_kind.add_null();
                            }
                            Some(kind) => {
                                let non_null = !kind.contains_null();

                                // If this `Kind` cannot be null, then the entire coalesced segment
                                // will never be null, so we have to remove any reference to it.
                                if non_null {
                                    // Can error if we're left with an empty kind after removing
                                    // `null`, but that's okay in this case, as we deal with the
                                    // empty state at the end.
                                    if let Err(EmptyKindError) = merged_kind.remove_null() {
                                        merged_kind = Self::empty();
                                    }
                                }

                                merged_kind.merge(
                                    kind.into_owned(),
                                    merge::Strategy {
                                        depth: merge::Depth::Deep,
                                        indices: merge::Indices::Keep,
                                    },
                                );

                                // Additionally, we can abort the loop, as this variant will
                                // _always_ match at runtime.
                                if non_null {
                                    break;
                                }
                            }
                        };
                    }

                    return Ok(if merged_kind.is_empty() {
                        None
                    } else {
                        Some(Cow::Owned(merged_kind))
                    });
                }

                // Try finding the index in the existing array.
                Segment::Index(index) => {
                    match get_element_from_array(
                        kind,
                        usize::try_from(*index).map_err(|_| Error::NegativeIndexPath)?,
                    ) {
                        None => return Ok(None),
                        Some(InnerKind::Exact(kind)) => kind,
                        Some(InnerKind::Infinite(kind)) => {
                            return Ok(Some(Owned(if or_null { kind.or_null() } else { kind })))
                        }
                    }
                }
            };
        }

        Ok(Some(if or_null {
            Owned(kind.clone().or_null())
        } else {
            Borrowed(kind)
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use lookup::LookupBuf;

    use super::*;
    use crate::kind::Collection;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_find_at_path() {
        struct TestCase {
            kind: Kind,
            path: LookupBuf,
            want: Result<Option<Kind>, Error>,
        }

        for (title, TestCase { kind, path, want }) in HashMap::from([
            (
                "primitive",
                TestCase {
                    kind: Kind::bytes(),
                    path: "foo".into(),
                    want: Ok(None),
                },
            ),
            (
                "multiple primitives",
                TestCase {
                    kind: Kind::integer().or_regex(),
                    path: "foo".into(),
                    want: Ok(None),
                },
            ),
            (
                "object w/ matching path",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                    path: "foo".into(),
                    want: Ok(Some(Kind::integer())),
                },
            ),
            (
                "object w/ unknown, w/o matching path",
                TestCase {
                    kind: Kind::object({
                        let mut v =
                            Collection::from(BTreeMap::from([("foo".into(), Kind::integer())]));
                        v.set_unknown(Kind::boolean());
                        v
                    }),
                    path: "bar".into(),
                    want: Ok(Some(Kind::boolean())),
                },
            ),
            (
                "object w/o unknown, w/o matching path",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                    path: "bar".into(),
                    want: Ok(None),
                },
            ),
            (
                "array w/ matching path",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[1]").unwrap(),
                    want: Ok(Some(Kind::integer())),
                },
            ),
            (
                "array w/ unknown, w/o matching path",
                TestCase {
                    kind: Kind::array({
                        let mut v = Collection::from(BTreeMap::from([(1.into(), Kind::integer())]));
                        v.set_unknown(Kind::bytes());
                        v
                    }),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    want: Ok(Some(Kind::bytes())),
                },
            ),
            (
                "array w/o unknown, w/o matching path",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    want: Ok(None),
                },
            ),
            (
                "array w/ negative indexing",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[-1]").unwrap(),
                    want: Err(Error::NegativeIndexPath),
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
                    want: Ok(Some(Kind::object(BTreeMap::from([(
                        "baz".into(),
                        Kind::integer().or_regex(),
                    )])))),
                },
            ),
            (
                "unknown kind for missing object path",
                TestCase {
                    kind: Kind::object({
                        let mut v =
                            Collection::from(BTreeMap::from([("foo".into(), Kind::timestamp())]));
                        v.set_unknown(Kind::bytes().or_integer());
                        v
                    }),
                    path: LookupBuf::from_str(".nope").unwrap(),
                    want: Ok(Some(Kind::bytes().or_integer())),
                },
            ),
            (
                "unknown kind for missing array index",
                TestCase {
                    kind: Kind::array({
                        let mut v =
                            Collection::from(BTreeMap::from([(0.into(), Kind::timestamp())]));
                        v.set_unknown(Kind::regex().or_null());
                        v
                    }),
                    path: LookupBuf::from_str("[1]").unwrap(),
                    want: Ok(Some(Kind::regex().or_null())),
                },
            ),
            (
                "or null for nested nullable path",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])).or_null(),
                    path: "foo".into(),
                    want: Ok(Some(Kind::integer().or_null())),
                },
            ),
            (
                "coalesced segment folding",
                TestCase {
                    kind: Kind::object(BTreeMap::from([
                        ("foo".into(), Kind::integer().or_null()),
                        ("bar".into(), Kind::float()),
                    ])),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    want: Ok(Some(Kind::integer().or_float())),
                },
            ),
            (
                "coalesced segment nullable",
                TestCase {
                    kind: Kind::object(BTreeMap::from([
                        ("foo".into(), Kind::integer().or_null()),
                        ("bar".into(), Kind::float().or_null()),
                    ])),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    want: Ok(Some(Kind::integer().or_float().or_null())),
                },
            ),
            (
                "coalesced segment early-match",
                TestCase {
                    kind: Kind::object(BTreeMap::from([
                        ("foo".into(), Kind::integer()),
                        ("bar".into(), Kind::float()),
                    ])),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    want: Ok(Some(Kind::integer())),
                },
            ),
            (
                "coalesced segment exact-null",
                TestCase {
                    kind: Kind::object(BTreeMap::from([
                        ("foo".into(), Kind::integer()),
                        ("bar".into(), Kind::float()),
                    ])),
                    path: LookupBuf::from_str(".(baz | foo | bar)").unwrap(),
                    want: Ok(Some(Kind::integer())),
                },
            ),
            (
                "coalesced segment multiple objects",
                TestCase {
                    kind: Kind::object(BTreeMap::from([
                        (
                            "foo".into(),
                            Kind::null().or_object(BTreeMap::from([
                                ("one".into(), Kind::integer()),
                                ("two".into(), Kind::integer()),
                            ])),
                        ),
                        (
                            "bar".into(),
                            Kind::object(BTreeMap::from([
                                ("two".into(), Kind::boolean()),
                                ("three".into(), Kind::boolean()),
                            ])),
                        ),
                    ])),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    // TODO(Jean):
                    //
                    // This isn't as accurate as it can be, but it is accurate enough to pass all
                    // existing tests. In reality, we should track two different `object` states
                    // for the kind, one for each potential coalesced arm. For this to work, the
                    // internal API of `Kind` has to be updated to store `BTreeSet<Collection<T>>`,
                    // instead of the existing `Option<Collection<T>>`.
                    want: Ok(Some(Kind::object(BTreeMap::from([
                        ("one".into(), Kind::integer()),
                        ("two".into(), Kind::integer().or_boolean()),
                        ("three".into(), Kind::boolean()),
                    ])))),
                },
            ),
            (
                "coalesced segment null arms",
                TestCase {
                    kind: Kind::object(BTreeMap::from([
                        ("foo".into(), Kind::null()),
                        ("bar".into(), Kind::null()),
                    ])),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    want: Ok(Some(Kind::null())),
                },
            ),
            (
                "no matching arms",
                TestCase {
                    kind: Kind::object(BTreeMap::from([
                        ("foo".into(), Kind::null()),
                        ("bar".into(), Kind::null()),
                    ])),
                    path: LookupBuf::from_str(".(baz | qux)").unwrap(),
                    want: Ok(Some(Kind::null())),
                },
            ),
        ]) {
            assert_eq!(
                kind.find_at_path(&path.to_lookup())
                    .map(|v| v.map(std::borrow::Cow::into_owned)),
                want,
                "returned: {}",
                title
            );
        }
    }
}
