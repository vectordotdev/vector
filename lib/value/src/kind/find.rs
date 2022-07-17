//! All types related to finding a [`Kind`] nested into another one.

use std::{borrow::Cow, collections::VecDeque};

use lookup::lookup_v2::{BorrowedSegment, Path};
use lookup::{Field, Lookup, Segment};

use super::Kind;
use crate::kind::merge;

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
                            unknown
                                .as_exact()
                                .map(InnerKind::Exact)
                                .or_else(|| Some(InnerKind::Infinite(unknown.to_kind())))
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
                            unknown
                                .as_exact()
                                .map(InnerKind::Exact)
                                .or_else(|| Some(InnerKind::Infinite(unknown.to_kind())))
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
                    let mut merged_kind = Self::never();

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
                                    // if the type is empty after removing null, we deal with it at the end.
                                    merged_kind.remove_null();
                                }

                                merged_kind.merge(
                                    kind.into_owned(),
                                    merge::Strategy {
                                        collisions: merge::CollisionStrategy::Union,
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

                    return Ok(if merged_kind.is_never() {
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

    /// Insert the `Kind` at the given `path` within `self`.
    /// This has the same behavior as `Value::get`.
    pub fn get<'a>(&self, path: impl Path<'a>) -> Kind {
        self.get_recursive(path.segment_iter())
    }

    fn get_field<'a>(&self, field: Cow<'a, str>) -> Kind {
        if let Some(object) = self.as_object() {
            let mut kind = object
                .known()
                .get(&field.into_owned().into())
                .cloned()
                .unwrap_or_else(|| object.unknown_kind());

            if !self.is_exact() {
                kind = kind.or_undefined();
            }
            kind
        } else {
            Kind::undefined()
        }
    }

    fn get_recursive<'a>(
        &self,
        mut iter: impl Iterator<Item = BorrowedSegment<'a>> + Clone,
    ) -> Kind {
        if self.is_never() {
            // a terminating expression by definition can "never" resolve to a value
            return Kind::never();
        }

        match iter.next() {
            Some(BorrowedSegment::Field(field)) | Some(BorrowedSegment::CoalesceEnd(field)) => {
                self.get_field(field).get_recursive(iter)
            }
            Some(BorrowedSegment::Index(mut index)) => {
                if let Some(array) = self.as_array() {
                    if index < 0 {
                        let largest_known_index = array.known().keys().map(|i| i.to_usize()).max();
                        // the minimum size of the resulting array
                        let len_required = -index as usize;

                        if array.unknown_kind().contains_any_defined() {
                            // the exact length is not known. We can't know for sure if the index
                            // will point to a known or unknown type, so the union of the unknown type
                            // plus any possible known type must be taken. Just the unknown type alone is not sufficient

                            // the array may be larger, but this is the largest we can prove the array is from the type information
                            let min_length = largest_known_index.map_or(0, |i| i + 1);

                            // We can prove the positive index won't be less than "min_index"
                            let min_index = (min_length as isize + index).max(0) as usize;
                            let can_underflow = (min_length as isize + index) < 0;

                            let mut kind = array.unknown_kind();

                            // We can prove the index won't underflow, so it cannot be "undefined".
                            // But only if the type can only be an array.
                            if self.is_exact() && !can_underflow {
                                kind.remove_undefined();
                            }

                            for (i, i_kind) in array.known() {
                                if i.to_usize() >= min_index {
                                    kind.merge_keep(i_kind.clone(), false);
                                }
                            }
                            return kind.get_recursive(iter);
                        } else {
                            // there are no unknown indices, so we can determine the exact positive index
                            let exact_len = largest_known_index.map_or(0, |x| x + 1);
                            if exact_len >= len_required {
                                // make the index positive, then continue below
                                index += exact_len as isize;
                            } else {
                                // out of bounds index
                                return Kind::undefined();
                            }
                        }
                    }

                    debug_assert!(index >= 0, "negative indices already handled");

                    let index = index as usize;
                    let mut kind = array
                        .known()
                        .get(&index.into())
                        .cloned()
                        .unwrap_or_else(|| array.unknown_kind());

                    if !self.is_exact() {
                        kind = kind.or_undefined();
                    }
                    kind.get_recursive(iter)
                } else {
                    Kind::undefined()
                }
            }
            Some(BorrowedSegment::CoalesceField(field)) => {
                let field_kind = self.get_field(field);

                // the remaining segments if this match succeeded
                let match_iter = iter
                    .clone()
                    .skip_while(|segment| matches!(segment, BorrowedSegment::CoalesceField(_)))
                    // skip the CoalesceEnd, which always exists after CoalesceFields
                    .skip(1)
                    // need to collect to prevent infinite recursive iterator type
                    .collect::<Vec<_>>();

                // This is the resulting type, assuming the match succeeded.
                let match_type = field_kind
                    .clone()
                    // This type is only valid if the match succeeded, which means this type wasn't undefined
                    .without_undefined()
                    .get_recursive(match_iter.into_iter());

                if !field_kind.contains_undefined() {
                    // this coalesce field will always be defined, so skip the others.
                    return match_type;
                }

                // the first field may or may not succeed. Try both.
                self.get_recursive(iter).union(match_type)
            }
            Some(BorrowedSegment::Invalid) => {
                // Value::get returns `None` in this case, which means the value is not defined
                Kind::undefined()
            }
            None => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use lookup::lookup_v2::{parse_path, OwnedPath};
    use lookup::owned_path;
    use std::collections::{BTreeMap, HashMap};

    use lookup::LookupBuf;

    use super::*;
    use crate::kind::Collection;
    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_get() {
        struct TestCase {
            kind: Kind,
            path: OwnedPath,
            want: Kind,
        }

        for (title, TestCase { kind, path, want }) in HashMap::from([
            (
                "get root",
                TestCase {
                    kind: Kind::bytes(),
                    path: owned_path!(),
                    want: Kind::bytes(),
                },
            ),
            (
                "get field from non-object",
                TestCase {
                    kind: Kind::bytes(),
                    path: owned_path!("foo"),
                    want: Kind::undefined(),
                },
            ),
            (
                "get field from object",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                    path: owned_path!("a"),
                    want: Kind::integer(),
                },
            ),
            (
                "get field from maybe an object",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])).or_null(),
                    path: owned_path!("a"),
                    want: Kind::integer().or_undefined(),
                },
            ),
            (
                "get unknown from object with no unknown",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                    path: owned_path!("b"),
                    want: Kind::undefined(),
                },
            ),
            (
                "get unknown from object with unknown",
                TestCase {
                    kind: Kind::object(
                        Collection::from(BTreeMap::from([("a".into(), Kind::integer())]))
                            .with_unknown(Kind::bytes()),
                    ),
                    path: owned_path!("b"),
                    want: Kind::bytes().or_undefined(),
                },
            ),
            (
                "get unknown from object with null unknown",
                TestCase {
                    kind: Kind::object(
                        Collection::from(BTreeMap::from([("a".into(), Kind::integer())]))
                            .with_unknown(Kind::null()),
                    ),
                    path: owned_path!("b"),
                    want: Kind::null().or_undefined(),
                },
            ),
            (
                "get nested field",
                TestCase {
                    kind: Kind::object(
                        Collection::from(BTreeMap::from([(
                            "a".into(),
                            Kind::object(
                                Collection::from(BTreeMap::from([("b".into(), Kind::integer())]))
                                    .with_unknown(Kind::null()),
                            ),
                        )]))
                        .with_unknown(Kind::null()),
                    ),
                    path: owned_path!("a", "b"),
                    want: Kind::integer(),
                },
            ),
            (
                "get index from non-array",
                TestCase {
                    kind: Kind::bytes(),
                    path: owned_path!(1),
                    want: Kind::undefined(),
                },
            ),
            (
                "get index from array",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                    path: owned_path!(0),
                    want: Kind::integer(),
                },
            ),
            (
                "get index from maybe array",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])).or_bytes(),
                    path: owned_path!(0),
                    want: Kind::integer().or_undefined(),
                },
            ),
            (
                "get unknown from array with no unknown",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                    path: owned_path!(1),
                    want: Kind::undefined(),
                },
            ),
            (
                "get unknown from array with unknown",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::integer())]))
                            .with_unknown(Kind::bytes()),
                    ),
                    path: owned_path!(1),
                    want: Kind::bytes().or_undefined(),
                },
            ),
            (
                "get unknown from array with null unknown",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::integer())]))
                            .with_unknown(Kind::null()),
                    ),
                    path: owned_path!(1),
                    want: Kind::null().or_undefined(),
                },
            ),
            (
                "get nested index",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([(
                            0.into(),
                            Kind::array(
                                Collection::from(BTreeMap::from([(0.into(), Kind::integer())]))
                                    .with_unknown(Kind::null()),
                            ),
                        )]))
                        .with_unknown(Kind::null()),
                    ),
                    path: owned_path!(0, 0),
                    want: Kind::integer(),
                },
            ),
            (
                "out of bounds negative index",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::integer(),
                    )]))),
                    path: owned_path!(-2),
                    want: Kind::undefined(),
                },
            ),
            (
                "negative index no unknown",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::integer(),
                    )]))),
                    path: owned_path!(-1),
                    want: Kind::integer(),
                },
            ),
            (
                "negative index with unknown can't underflow",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::integer()),
                            (1.into(), Kind::bytes()),
                            (2.into(), Kind::float()),
                        ]))
                        .with_unknown(Kind::boolean()),
                    ),
                    path: owned_path!(-2),
                    want: Kind::boolean().or_bytes().or_float(),
                },
            ),
            (
                "negative index with unknown can't underflow (maybe array)",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::integer()),
                            (1.into(), Kind::bytes()),
                            (2.into(), Kind::float()),
                        ]))
                        .with_unknown(Kind::boolean()),
                    )
                    .or_null(),
                    path: owned_path!(-2),
                    want: Kind::boolean().or_bytes().or_float().or_undefined(),
                },
            ),
            (
                "negative index with unknown can underflow",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::integer())]))
                            .with_unknown(Kind::boolean()),
                    ),
                    path: owned_path!(-2),
                    want: Kind::boolean().or_integer().or_undefined(),
                },
            ),
            (
                "negative index nested",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::integer())]))
                            .with_unknown(Kind::object(BTreeMap::from([(
                                "foo".into(),
                                Kind::bytes(),
                            )]))),
                    ),
                    path: owned_path!(-2, "foo"),
                    want: Kind::bytes().or_undefined(),
                },
            ),
            (
                "coalesce first defined, no unknown",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_path(".(a|b)"),
                    want: Kind::integer(),
                },
            ),
            (
                "coalesce 2nd defined, no unknown",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "b".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_path(".(a|b)"),
                    want: Kind::integer(),
                },
            ),
            (
                "coalesce first defined, unknown",
                TestCase {
                    kind: Kind::object(
                        Collection::from(BTreeMap::from([("a".into(), Kind::integer())]))
                            .with_unknown(Kind::bytes()),
                    ),
                    path: parse_path(".(a|b)"),
                    want: Kind::integer(),
                },
            ),
            (
                "coalesce 2nd defined, unknown",
                TestCase {
                    kind: Kind::object(
                        Collection::from(BTreeMap::from([("b".into(), Kind::integer())]))
                            .with_unknown(Kind::bytes()),
                    ),
                    path: parse_path(".(a|b)"),
                    want: Kind::bytes().or_integer(),
                },
            ),
            (
                "coalesce nested",
                TestCase {
                    kind: Kind::object(
                        Collection::from(BTreeMap::from([("b".into(), Kind::integer())]))
                            .with_unknown(Kind::object(BTreeMap::from([(
                                "foo".into(),
                                Kind::bytes(),
                            )]))),
                    ),
                    path: parse_path(".(a|b).foo"),
                    want: Kind::bytes().or_undefined(),
                },
            ),
            (
                "nested terminating expression",
                TestCase {
                    kind: Kind::never(),
                    path: owned_path!(".foo.bar"),
                    want: Kind::never(),
                },
            ),
        ]) {
            println!("========== {} ==========", title);
            assert_eq!(kind.get(&path), want, "test: {}", title);
        }
    }

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
                    // TODO: This is returning a type more general than it could be, but it is "correct"
                    //       otherwise
                    // The type system currently doesn't distinguish between "null" and "undefined",
                    // but the runtime behavior of coalescing _does_, so in general types
                    // for coalesced paths won't be as accurate as possible.
                    // (This specific example could be fixed though)
                    want: Ok(Some(Kind::object(BTreeMap::from([
                        ("one".into(), Kind::integer().or_null()),
                        ("two".into(), Kind::integer().or_boolean()),
                        ("three".into(), Kind::boolean().or_null()),
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
