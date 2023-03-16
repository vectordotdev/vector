//! All types related to inserting one [`Kind`] into another.

use lookup::lookup_v2::{BorrowedSegment, ValuePath};

use crate::kind::Collection;
use crate::Kind;
use lookup::path;

impl Kind {
    /// Insert the `Kind` at the given `path` within `self`.
    /// This has the same behavior as `Value::insert`.
    #[allow(clippy::needless_pass_by_value)] // only reference types implement Path
    pub fn insert<'a>(&'a mut self, path: impl ValuePath<'a>, kind: Self) {
        self.insert_recursive(path.segment_iter(), kind.upgrade_undefined());
    }

    /// Set the `Kind` at the given `path` within `self`.
    /// There is a subtle difference
    /// between this and `Kind::insert` where this function does _not_ convert undefined to null.
    #[allow(clippy::needless_pass_by_value)] // only reference types implement Path
    pub fn set_at_path<'a>(&'a mut self, path: impl ValuePath<'a>, kind: Self) {
        self.insert_recursive(path.segment_iter(), kind);
    }

    /// Insert the `Kind` at the given `path` within `self`.
    /// This has the same behavior as `Value::insert`.
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::needless_pass_by_value)] // only reference types implement Path
    pub fn insert_recursive<'a, 'b>(
        &'a mut self,
        mut iter: impl Iterator<Item = BorrowedSegment<'b>> + Clone,
        kind: Self,
    ) {
        if self.is_never() || kind.is_never() {
            // If `self` or `kind` is `never`, the program would have already terminated
            // so this assignment can't happen.
            *self = Self::never();
            return;
        }

        if let Some(segment) = iter.next() {
            match segment {
                BorrowedSegment::Field(field) | BorrowedSegment::CoalesceEnd(field) => {
                    // Field insertion converts the value to an object, so remove all other types.
                    *self = Self::object(self.object.clone().unwrap_or_else(Collection::empty));

                    let collection = self.object.as_mut().expect("object was just inserted");
                    let unknown_kind = collection.unknown_kind();

                    collection
                        .known_mut()
                        .entry(field.into_owned().into())
                        .or_insert(unknown_kind)
                        .insert_recursive(iter, kind);
                }
                BorrowedSegment::Index(mut index) => {
                    // Array insertion converts the value to an array, so remove all other types.
                    *self = Self::array(self.array.clone().unwrap_or_else(Collection::empty));
                    let collection = self.array.as_mut().expect("array was just inserted");

                    if index < 0 {
                        let largest_known_index = collection.largest_known_index();
                        // The minimum size of the resulting array.
                        let len_required = -index as usize;

                        let unknown_kind = collection.unknown_kind();
                        if unknown_kind.contains_any_defined() {
                            // The array may be larger, but this is the largest we can prove the array is from the type information.
                            let min_length = collection.min_length();

                            if len_required > min_length {
                                // We can't prove the array is large enough, so "holes" may be created
                                // which set the value to null.
                                // Holes are inserted to the front, which shifts everything to the right.
                                // We don't know the exact number of holes/shifts, but can determine an upper bound.
                                let max_shifts = len_required - min_length;

                                // The number of possible shifts is 0 ..= max_shifts.
                                // Each shift will be calculated independently and merged into the collection.
                                // A shift of 0 is the original collection, so that is skipped.
                                let zero_shifts = collection.clone();
                                for shift_count in 1..=max_shifts {
                                    let mut shifted_collection = zero_shifts.clone();
                                    // Clear all known values and replace with new ones. (in-place shift can overwrite).
                                    shifted_collection.known_mut().clear();

                                    // Add the "null" from holes.
                                    for i in 1..shift_count {
                                        shifted_collection
                                            .known_mut()
                                            .insert(i.into(), Self::null());
                                    }

                                    // Shift known values by the exact "shift_count".
                                    for (i, i_kind) in zero_shifts.known() {
                                        shifted_collection
                                            .known_mut()
                                            .insert(*i + shift_count, i_kind.clone());
                                    }

                                    // Add this shift count as another possible type definition.
                                    collection.merge(shifted_collection, false);
                                }
                            }

                            // We can prove the positive index won't be less than "min_index".
                            let min_index = (min_length as isize + index).max(0) as usize;

                            // Sanity check: if holes are added to the type, min_index must be 0.
                            debug_assert!(min_index == 0 || min_length >= len_required);

                            // Apply the current "unknown" to indices that don't have an explicit known
                            // since the "unknown" is about to change.
                            for i in 0..len_required {
                                collection
                                    .known_mut()
                                    .entry(i.into())
                                    .or_insert_with(|| unknown_kind.clone())
                                    // These indices are guaranteed to exist, so they can't be undefined.
                                    .remove_undefined();
                            }
                            for (i, i_kind) in collection.known_mut() {
                                // This index might be set by the insertion. Add the insertion type to the existing type.
                                if i.to_usize() >= min_index {
                                    let mut kind_with_insertion = i_kind.clone();
                                    let remaining_path_segments = iter.clone().collect::<Vec<_>>();
                                    kind_with_insertion
                                        .insert(&remaining_path_segments, kind.clone());
                                    *i_kind = i_kind.union(kind_with_insertion);
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
                            collection.unknown_kind().is_undefined(),
                            "all cases with an unknown have been handled"
                        );

                        // If there is no unknown, the exact position of the negative index can be determined.
                        let exact_array_len =
                            largest_known_index.map_or(0, |max_index| max_index + 1);

                        if len_required > exact_array_len {
                            // Fill in holes from extending to fit a negative index.
                            for i in exact_array_len..len_required {
                                // There is no unknown, so the exact type "null" can be inserted.
                                collection.known_mut().insert(i.into(), Self::null());
                            }
                        }
                        index += (len_required as isize).max(exact_array_len as isize);
                    }

                    debug_assert!(index >= 0, "all negative cases have been handled");
                    let index = index as usize;

                    let index_exists = collection.known().contains_key(&index.into());
                    if !index_exists {
                        // Add "null" to all holes, adding it to the "unknown" if it exists.
                        // Holes can never be undefined.
                        let hole_type = collection.unknown_kind().without_undefined().or_null();

                        for i in 0..index {
                            collection
                                .known_mut()
                                .entry(i.into())
                                .or_insert_with(|| hole_type.clone());
                        }
                    }

                    let unknown_kind = collection.unknown_kind();
                    collection
                        .known_mut()
                        .entry(index.into())
                        .or_insert(unknown_kind)
                        .insert_recursive(iter, kind);
                }
                BorrowedSegment::CoalesceField(field) => {
                    let field_kind = self.at_path(path!(field.as_ref()));
                    if field_kind.is_undefined() {
                        // this field is guaranteed to never match. Just skip it
                        return self.insert_recursive(iter, kind);
                    }

                    // Need to collect to prevent infinite recursive iterator type.
                    #[allow(clippy::needless_collect)]
                    // The remaining segments if this match succeeded.
                    let match_iter = iter
                        .clone()
                        .skip_while(|segment| matches!(segment, BorrowedSegment::CoalesceField(_)))
                        // Skip the `CoalesceEnd`, which always exists after `CoalesceFields`.
                        .skip(1)
                        .collect::<Vec<_>>();

                    // This is the resulting type, assuming the match succeeded.
                    let mut match_type = self.clone();
                    if let Some(object) = match_type.as_object_mut() {
                        if let Some(field_kind) = object.known_mut().get_mut(&field.as_ref().into())
                        {
                            // "match_type" is only valid if the match succeeded, which means this field wasn't undefined.
                            field_kind.remove_undefined();
                            field_kind.insert_recursive(match_iter.into_iter(), kind.clone());
                        }
                    }

                    if !field_kind.contains_undefined() {
                        // This coalesce field will always be defined, so skip the others.
                        *self = match_type;
                        return;
                    }

                    // The first field may or may not succeed. Try both.
                    self.insert_recursive(iter, kind);
                    *self = self.clone().union(match_type);
                }
                BorrowedSegment::Invalid => { /* An invalid path does nothing. */ }
            };
        } else {
            *self = kind;
        }
    }
}

#[cfg(test)]
mod tests {
    use lookup::lookup_v2::{parse_value_path, OwnedValuePath};
    use lookup::owned_value_path;
    use std::collections::BTreeMap;

    use super::*;
    use crate::kind::Collection;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_insert() {
        struct TestCase {
            this: Kind,
            path: OwnedValuePath,
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
        ) in [
            (
                "root insert",
                TestCase {
                    this: Kind::bytes(),
                    path: owned_value_path!(),
                    kind: Kind::integer(),
                    expected: Kind::integer(),
                },
            ),
            (
                "root insert object",
                TestCase {
                    this: Kind::bytes(),
                    path: owned_value_path!(),
                    kind: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "empty object insert field",
                TestCase {
                    this: Kind::object(Collection::empty()),
                    path: owned_value_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "non-empty object insert field",
                TestCase {
                    this: Kind::object(BTreeMap::from([("b".into(), Kind::bytes())])),
                    path: owned_value_path!("a"),
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
                    path: owned_value_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "set array index on empty array",
                TestCase {
                    this: Kind::array(Collection::empty()),
                    path: owned_value_path!(0),
                    kind: Kind::integer(),
                    expected: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                },
            ),
            (
                "set array index past the end without unknown",
                TestCase {
                    this: Kind::array(Collection::empty()),
                    path: owned_value_path!(1),
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
                    path: owned_value_path!(1),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::integer().or_null()),
                            (1.into(), Kind::bytes()),
                        ]))
                        .with_unknown(Kind::integer()),
                    ),
                },
            ),
            (
                "set array index past the end with unknown, nested",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_value_path!(1, "foo"),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::integer().or_null()),
                            (
                                1.into(),
                                Kind::object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                            ),
                        ]))
                        .with_unknown(Kind::integer()),
                    ),
                },
            ),
            (
                "set array index past the end with null unknown",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::null())),
                    path: owned_value_path!(1),
                    kind: Kind::integer(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::null()),
                            (1.into(), Kind::integer()),
                        ]))
                        .with_unknown(Kind::null()),
                    ),
                },
            ),
            (
                "set field on non-object",
                TestCase {
                    this: Kind::integer(),
                    path: owned_value_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "set array index on non-array",
                TestCase {
                    this: Kind::integer(),
                    path: owned_value_path!(0),
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
                    path: owned_value_path!(-1),
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
                    path: owned_value_path!(-2),
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
                    path: owned_value_path!(-1),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::bytes().or_integer())]))
                            .with_unknown(Kind::integer().or_bytes().or_undefined()),
                    ),
                },
            ),
            (
                "set negative array index empty unknown array",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_value_path!(-1),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            // we can prove the first index will not be undefined
                            (0.into(), Kind::bytes().or_integer()),
                        ]))
                        .with_unknown(Kind::integer().or_bytes().or_undefined()),
                    ),
                },
            ),
            (
                "set negative array index empty unknown array (2)",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_value_path!(-2),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::integer().or_bytes()),
                            // This is the only location a hole could potentially be inserted, so it
                            // is the only index that gets "null", rather than adding it to the
                            // entire unknown type.
                            (1.into(), Kind::integer().or_bytes().or_null()),
                        ]))
                        .with_unknown(Kind::integer().or_bytes().or_undefined()),
                    ),
                },
            ),
            (
                "set negative array index unknown array",
                TestCase {
                    this: Kind::array(
                        Collection::from(BTreeMap::from([
                            // This would be an invalid type without index 0 (it can't be undefined).
                            (0.into(), Kind::integer()),
                            (1.into(), Kind::float()),
                        ]))
                        .with_unknown(Kind::integer()),
                    ),
                    path: owned_value_path!(-3),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            // Either the unknown (integer) or the inserted value, depending on the actual length.
                            (0.into(), Kind::integer().or_bytes()),
                            // The original float if it wasn't shifted, or bytes/integer if it was shifted.
                            // Can't be a hole.
                            (1.into(), Kind::float().or_bytes().or_integer()),
                            (2.into(), Kind::float().or_bytes().or_integer()),
                        ]))
                        .with_unknown(Kind::integer().or_bytes().or_undefined()),
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
                    path: owned_value_path!(-3),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::float().or_bytes()),
                            (1.into(), Kind::float().or_bytes()),
                            (2.into(), Kind::float().or_bytes()),
                        ]))
                        .with_unknown(Kind::integer().or_bytes().or_undefined()),
                    ),
                },
            ),
            (
                "set negative array index on non-array",
                TestCase {
                    this: Kind::integer(),
                    path: owned_value_path!(-3),
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
                    path: owned_value_path!(-3, "foo"),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (
                                0.into(),
                                Kind::integer()
                                    .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                            ),
                            (
                                1.into(),
                                Kind::integer()
                                    .or_null()
                                    .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                            ),
                            (
                                2.into(),
                                Kind::integer()
                                    .or_null()
                                    .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                            ),
                        ]))
                        .with_unknown(
                            Kind::integer()
                                .or_undefined()
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
                    path: owned_value_path!(-1, "foo"),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(
                            0.into(),
                            Kind::integer()
                                .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                        )]))
                        .with_unknown(
                            Kind::integer()
                                .or_undefined()
                                .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                        ),
                    ),
                },
            ),
            (
                "coalesce empty object",
                TestCase {
                    this: Kind::object(Collection::empty()),
                    path: parse_value_path(".(a|b)").unwrap(),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([(
                        "b".into(),
                        Kind::bytes(),
                    )]))),
                },
            ),
            (
                "coalesce first exists",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_value_path(".(a|b)").unwrap(),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::bytes(),
                    )]))),
                },
            ),
            (
                "coalesce second exists",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([(
                        "b".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_value_path(".(a|b)").unwrap(),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([(
                        "b".into(),
                        Kind::bytes(),
                    )]))),
                },
            ),
            (
                "coalesce both exist",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer()),
                        ("b".into(), Kind::integer()),
                    ]))),
                    path: parse_value_path(".(a|b)").unwrap(),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::bytes()),
                        ("b".into(), Kind::integer()),
                    ]))),
                },
            ),
            (
                "coalesce nested",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([]))),
                    path: parse_value_path(".(a|b).x").unwrap(),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([(
                        "b".into(),
                        Kind::object(BTreeMap::from([("x".into(), Kind::bytes())])),
                    )]))),
                },
            ),
            (
                "insert into never",
                TestCase {
                    this: Kind::never(),
                    path: parse_value_path(".x").unwrap(),
                    kind: Kind::bytes(),
                    expected: Kind::never(),
                },
            ),
            (
                "insert never",
                TestCase {
                    this: Kind::object(Collection::empty()),
                    path: parse_value_path(".x").unwrap(),
                    kind: Kind::never(),
                    expected: Kind::never(),
                },
            ),
            (
                "insert undefined",
                TestCase {
                    this: Kind::object(Collection::empty()),
                    path: parse_value_path(".x").unwrap(),
                    kind: Kind::undefined(),
                    expected: Kind::object(BTreeMap::from([("x".into(), Kind::null())])),
                },
            ),
            (
                "array insert into any",
                TestCase {
                    this: Kind::any(),
                    path: owned_value_path!(2),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::any().without_undefined()),
                            (1.into(), Kind::any().without_undefined()),
                            (2.into(), Kind::bytes()),
                        ]))
                        .with_unknown(Kind::any()),
                    ),
                },
            ),
            (
                "object insert into any",
                TestCase {
                    this: Kind::any(),
                    path: owned_value_path!("b"),
                    kind: Kind::bytes(),
                    expected: Kind::object(
                        Collection::from(BTreeMap::from([("b".into(), Kind::bytes())]))
                            .with_unknown(Kind::any()),
                    ),
                },
            ),
            (
                "nested object/array insert into any",
                TestCase {
                    this: Kind::any(),
                    path: owned_value_path!("x", 2),
                    kind: Kind::bytes(),
                    expected: Kind::object(
                        Collection::from(BTreeMap::from([(
                            "x".into(),
                            Kind::array(
                                Collection::from(BTreeMap::from([
                                    (0.into(), Kind::any().without_undefined()),
                                    (1.into(), Kind::any().without_undefined()),
                                    (2.into(), Kind::bytes()),
                                ]))
                                .with_unknown(Kind::any()),
                            ),
                        )]))
                        .with_unknown(Kind::any()),
                    ),
                },
            ),
            (
                "nested array/array insert into any",
                TestCase {
                    this: Kind::any(),
                    path: owned_value_path!(0, 0),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(
                            0.into(),
                            Kind::array(
                                Collection::from(BTreeMap::from([(0.into(), Kind::bytes())]))
                                    .with_unknown(Kind::any()),
                            ),
                        )]))
                        .with_unknown(Kind::any()),
                    ),
                },
            ),
        ] {
            this.insert(&path, kind);
            assert_eq!(this, expected, "{title}");
        }
    }
}
