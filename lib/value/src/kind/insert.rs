//! All types related to inserting one [`Kind`] into another.

use lookup::lookup_v2::{BorrowedSegment, Path};
use std::collections::btree_map::Entry;

use super::Kind;
use crate::kind::{Collection, Unknown};
use lookup::path;

impl Kind {
    /// Insert the `Kind` at the given `path` within `self`.
    /// This has the same behavior as `Value::insert`.
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::needless_pass_by_value)] // only reference types implement Path
    pub fn insert<'a>(&'a mut self, path: impl Path<'a>, kind: Self) {
        // need to re-bind self to make a mutable reference
        let mut self_kind = self;

        let mut iter = path.segment_iter().peekable();

        while let Some(segment) = iter.next() {
            self_kind = match segment {
                BorrowedSegment::Field(field) => {
                    // field insertion converts the value to an object, so remove all other types
                    *self_kind =
                        Self::object(self_kind.object.clone().unwrap_or_else(Collection::empty));
                    let collection = self_kind.object.as_mut().expect("object was just inserted");

                    match iter.peek() {
                        Some(segment) => {
                            match collection.known_mut().entry(field.into_owned().into()) {
                                Entry::Occupied(entry) => entry.into_mut(),
                                Entry::Vacant(entry) => entry.insert(Self::null()),
                            }
                        }
                        None => {
                            collection
                                .known_mut()
                                .insert(field.into_owned().into(), kind);
                            return;
                        }
                    }
                }
                BorrowedSegment::Index(mut index) => {
                    // array insertion converts the value to an array, so remove all other types
                    *self_kind =
                        Self::array(self_kind.array.clone().unwrap_or_else(Collection::empty));
                    let collection = self_kind.array.as_mut().expect("array was just inserted");

                    if index < 0 {
                        let largest_known_index =
                            collection.known().keys().map(|i| i.to_usize()).max();
                        // the minimum size of the resulting array
                        let len_required = -index as usize;

                        if let Some(unknown) = collection.unknown() {
                            let unknown_kind = unknown.to_kind();

                            // the array may be larger, but this is the largest we can prove the array is from the type information
                            let min_length = largest_known_index.map_or(0, |i| i + 1);

                            if len_required > min_length {
                                // We can't prove the array is large enough, so "holes" may be created
                                // which set the value to null.
                                // Holes are inserted to the front, which shifts everything to the right.
                                // We don't know the exact number of holes, but can determine an upper bound
                                let max_shifts = len_required - min_length;

                                // The number of possible shifts is 0 ..= max_shifts.
                                // Each shift will be calculated independently and merged into the collection.
                                // A shift of 0 is the original collection, so that is skipped
                                for shift_count in 1..=max_shifts {
                                    let mut shifted_collection = collection.clone();
                                    // clear all known values and replace with new ones. (in-place shift can overwrite)
                                    shifted_collection.known_mut().clear();

                                    // add the "null" from holes. Index 0 is handled below
                                    for i in 1..shift_count {
                                        shifted_collection
                                            .known_mut()
                                            .insert(i.into(), Self::null());
                                    }

                                    // Index 0 is always the inserted value if shifts are happening
                                    let mut item = Self::null();
                                    item.insert(&iter.clone().collect::<Vec<_>>(), kind.clone());
                                    shifted_collection.known_mut().insert(0.into(), item);

                                    // shift known values by the exact "shift_count"
                                    for (i, i_kind) in collection.known() {
                                        shifted_collection
                                            .known_mut()
                                            .insert(*i + shift_count, i_kind.clone());
                                    }

                                    // add this shift count as another possible type definition
                                    collection.merge(shifted_collection, false);
                                }
                            }

                            // We can prove the positive index won't be less than "min_index"
                            let min_index = (min_length as isize + index).max(0) as usize;

                            // sanity check: if holes are added to the type, min_index must be 0
                            debug_assert!(min_index == 0 || min_length >= len_required);

                            // indices less than the minimum possible index won't change.
                            // Apply the current "unknown" to indices that don't have an explicit known
                            // since the "unknown" is about to change
                            for i in 0..min_index {
                                if !collection.known().contains_key(&i.into()) {
                                    collection
                                        .known_mut()
                                        .insert(i.into(), unknown_kind.clone());
                                }
                            }
                            for (i, i_kind) in collection.known_mut() {
                                // This index might be set by the insertion, add the insertion type to the existing type
                                if i.to_usize() >= min_index {
                                    let mut kind_with_insertion = i_kind.clone();
                                    let remaining_path_segments = iter.clone().collect::<Vec<_>>();
                                    kind_with_insertion
                                        .insert(&remaining_path_segments, kind.clone());
                                    i_kind.merge_keep(kind_with_insertion, false);
                                }
                            }

                            let mut unknown_kind_with_insertion = unknown_kind.clone();
                            let remaining_path_segments = iter.clone().collect::<Vec<_>>();
                            unknown_kind_with_insertion.insert(&remaining_path_segments, kind);
                            let mut new_unknown_kind = unknown_kind;
                            new_unknown_kind.merge_keep(unknown_kind_with_insertion, false);
                            collection.set_unknown(new_unknown_kind);

                            return;
                        }
                        debug_assert!(
                            collection.unknown().is_none(),
                            "all cases with an unknown have been handled"
                        );

                        // If there is no unknown, the exact position of the negative index can be determined
                        let exact_array_len =
                            largest_known_index.map_or(0, |max_index| max_index + 1);

                        if len_required > exact_array_len {
                            // fill in holes from extending to fit a negative index
                            for i in exact_array_len..len_required {
                                // there is no unknown, so the exact type "null" can be inserted
                                collection.known_mut().insert(i.into(), Self::null());
                            }
                        }
                        index += (len_required as isize).max(exact_array_len as isize);
                    }

                    debug_assert!(index >= 0, "all negative cases have been handled");
                    let index = index as usize;

                    match iter.peek() {
                        Some(segment) => match collection.known_mut().entry(index.into()) {
                            Entry::Occupied(entry) => entry.into_mut(),
                            Entry::Vacant(entry) => entry.insert(Self::null()),
                        },
                        None => {
                            collection.known_mut().insert(index.into(), kind);

                            // add "null" to all holes, adding it to the "unknown" if it exists
                            let hole_type = collection
                                .unknown()
                                .map_or(Self::never(), Unknown::to_kind)
                                .or_null();

                            for i in 0..index {
                                collection
                                    .known_mut()
                                    .entry(i.into())
                                    .or_insert_with(|| hole_type.clone());
                            }
                            return;
                        }
                    }
                }
                BorrowedSegment::CoalesceField(field) => {
                    // TODO: This can be improved once "undefined" is a type
                    //   https://github.com/vectordotdev/vector/issues/13459

                    let remaining_segments = iter
                        .clone()
                        .skip_while(|segment| matches!(segment, BorrowedSegment::CoalesceField(_)))
                        // next segment must be a coalesce end, which is skipped
                        .skip(1)
                        .collect::<Vec<_>>();

                    // we don't know for sure if this coalesce will succeed, so the insertion is merged with the original value
                    let mut maybe_inserted_kind = self_kind.clone();
                    maybe_inserted_kind.insert(
                        path!(&field.into_owned()).concat(&remaining_segments),
                        kind.clone(),
                    );
                    self_kind.merge_keep(maybe_inserted_kind, false);
                    self_kind
                }
                BorrowedSegment::CoalesceEnd(field) => {
                    // TODO: This can be improved once "undefined" is a type
                    //   https://github.com/vectordotdev/vector/issues/13459

                    let remaining_segments = iter.clone().collect::<Vec<_>>();

                    // we don't know for sure if this coalesce will succeed, so the insertion is merged with the original value
                    let mut maybe_inserted_kind = self_kind.clone();
                    maybe_inserted_kind
                        .insert(path!(&field.into_owned()).concat(&remaining_segments), kind);
                    self_kind.merge_keep(maybe_inserted_kind, false);
                    return;
                }
                BorrowedSegment::Invalid => return,
            };
        }
        *self_kind = kind;
    }
}

#[cfg(test)]
mod tests {
    use lookup::lookup_v2::{parse_path, OwnedPath};
    use lookup::owned_path;
    use std::collections::BTreeMap;
    use std::collections::HashMap;

    use super::*;
    use crate::kind::Collection;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_insert() {
        struct TestCase {
            this: Kind,
            path: OwnedPath,
            kind: Kind,
            expected: Kind,
        }

        for (
            title,
            TestCase {
                mut this,
                path,
                kind,
                expected,
            },
        ) in HashMap::from([
            (
                "root insert",
                TestCase {
                    this: Kind::bytes(),
                    path: owned_path!(),
                    kind: Kind::integer(),
                    expected: Kind::integer(),
                },
            ),
            (
                "root insert object",
                TestCase {
                    this: Kind::bytes(),
                    path: owned_path!(),
                    kind: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "empty object insert field",
                TestCase {
                    this: Kind::object(Collection::empty()),
                    path: owned_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "non-empty object insert field",
                TestCase {
                    this: Kind::object(BTreeMap::from([("b".into(), Kind::bytes())])),
                    path: owned_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([
                        ("a".into(), Kind::integer()),
                        ("b".into(), Kind::bytes()),
                    ])),
                },
            ),
            (
                "object overwrite field",
                TestCase {
                    this: Kind::object(BTreeMap::from([("a".into(), Kind::bytes())])),
                    path: owned_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "set array index on empty array",
                TestCase {
                    this: Kind::array(Collection::empty()),
                    path: owned_path!(0),
                    kind: Kind::integer(),
                    expected: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                },
            ),
            (
                "set array index past the end without unknown",
                TestCase {
                    this: Kind::array(Collection::empty()),
                    path: owned_path!(1),
                    kind: Kind::integer(),
                    expected: Kind::array(BTreeMap::from([
                        (0.into(), Kind::null()),
                        (1.into(), Kind::integer()),
                    ])),
                },
            ),
            (
                "set array index past the end with unknown",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_path!(1),
                    kind: Kind::integer(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(1.into(), Kind::integer())]))
                            .with_unknown(Kind::integer()),
                    ),
                },
            ),
            (
                "set array index past the end with null unknown",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::null())),
                    path: owned_path!(1),
                    kind: Kind::integer(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(1.into(), Kind::integer())]))
                            .with_unknown(Kind::null()),
                    ),
                },
            ),
            (
                "set field on non-object",
                TestCase {
                    this: Kind::integer(),
                    path: owned_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "set array index on non-array",
                TestCase {
                    this: Kind::integer(),
                    path: owned_path!(0),
                    kind: Kind::integer(),
                    expected: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                },
            ),
            (
                "set negative array index (no unknown)",
                TestCase {
                    this: Kind::array(BTreeMap::from([
                        (0.into(), Kind::integer()),
                        (1.into(), Kind::integer()),
                    ])),
                    path: owned_path!(-1),
                    kind: Kind::bytes(),
                    expected: Kind::array(BTreeMap::from([
                        (0.into(), Kind::integer()),
                        (1.into(), Kind::bytes()),
                    ])),
                },
            ),
            (
                "set negative array index past the end (no unknown)",
                TestCase {
                    this: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                    path: owned_path!(-2),
                    kind: Kind::bytes(),
                    expected: Kind::array(BTreeMap::from([
                        (0.into(), Kind::bytes()),
                        (1.into(), Kind::null()),
                    ])),
                },
            ),
            (
                "set negative array index size 1 unknown array",
                TestCase {
                    this: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::integer())]))
                            .with_unknown(Kind::integer()),
                    ),
                    path: owned_path!(-1),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::bytes().or_integer())]))
                            .with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index empty unknown array",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_path!(-1),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::empty().with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index empty unknown array (2)",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_path!(-2),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::empty().with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index unknown array",
                TestCase {
                    this: Kind::array(
                        Collection::from(BTreeMap::from([(1.into(), Kind::float())]))
                            .with_unknown(Kind::integer()),
                    ),
                    path: owned_path!(-3),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (1.into(), Kind::float().or_bytes().or_null().or_integer()),
                            (2.into(), Kind::float().or_bytes().or_null().or_integer()),
                        ]))
                        .with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index unknown array no holes",
                TestCase {
                    this: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::float()),
                            (1.into(), Kind::float()),
                            (2.into(), Kind::float()),
                        ]))
                        .with_unknown(Kind::integer()),
                    ),
                    path: owned_path!(-3),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::float().or_bytes()),
                            (1.into(), Kind::float().or_bytes()),
                            (2.into(), Kind::float().or_bytes()),
                        ]))
                        .with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index on non-array",
                TestCase {
                    this: Kind::integer(),
                    path: owned_path!(-3),
                    kind: Kind::bytes(),
                    expected: Kind::array(Collection::from(BTreeMap::from([
                        (0.into(), Kind::bytes()),
                        (1.into(), Kind::null()),
                        (2.into(), Kind::null()),
                    ]))),
                },
            ),
            (
                "set nested negative array index on unknown array",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_path!(-3, "foo"),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::empty().with_unknown(
                            Kind::integer()
                                .or_null()
                                .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                        ),
                    ),
                },
            ),
            (
                "set nested negative array index on unknown array (no holes)",
                TestCase {
                    this: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::integer())]))
                            .with_unknown(Kind::integer()),
                    ),
                    path: owned_path!(-1, "foo"),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(
                            0.into(),
                            Kind::integer()
                                .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                        )]))
                        .with_unknown(
                            Kind::integer()
                                .or_null()
                                .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                        ),
                    ),
                },
            ),
            (
                "coalesce empty object",
                TestCase {
                    this: Kind::object(Collection::empty()),
                    path: parse_path(".(a|b)"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::bytes().or_null()),
                        ("b".into(), Kind::bytes().or_null()),
                    ]))),
                },
            ),
            (
                "coalesce first exists",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_path(".(a|b)"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_bytes()),
                        ("b".into(), Kind::bytes().or_null()),
                    ]))),
                },
            ),
            (
                "coalesce second exists",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([(
                        "b".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_path(".(a|b)"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::bytes().or_null()),
                        ("b".into(), Kind::integer().or_bytes()),
                    ]))),
                },
            ),
            (
                "coalesce both exist",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer()),
                        ("b".into(), Kind::integer()),
                    ]))),
                    path: parse_path(".(a|b)"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_bytes()),
                        ("b".into(), Kind::integer().or_bytes()),
                    ]))),
                },
            ),
            (
                "coalesce nested",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([]))),
                    path: parse_path(".(a|b).x"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        (
                            "a".into(),
                            Kind::object(BTreeMap::from([("x".into(), Kind::bytes())])).or_null(),
                        ),
                        (
                            "b".into(),
                            Kind::object(BTreeMap::from([("x".into(), Kind::bytes())])).or_null(),
                        ),
                    ]))),
                },
            ),
        ]) {
            this.insert(&path, kind);
            assert_eq!(this, expected, "{}", title);
        }
    }
}
