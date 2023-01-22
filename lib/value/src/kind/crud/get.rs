//! All types related to finding a [`Kind`] nested into another one.

use crate::Kind;
use lookup::lookup_v2::{BorrowedSegment, ValuePath};
use std::borrow::Cow;

impl Kind {
    /// Returns the type of a value that is retrieved from a certain path.
    ///
    /// This has the same behavior as `Value::get`, including
    /// the implicit conversion of "undefined" to "null.
    ///
    /// If you want the type _without_ the implicit type conversion,
    /// use `Kind::at_path` instead.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // only references are implemented for `Path`
    pub fn get<'a>(&self, path: impl ValuePath<'a>) -> Self {
        self.at_path(path).upgrade_undefined()
    }

    /// This retrieves the `Kind` at a given path. There is a subtle difference
    /// between this and `Kind::get` where this function does _not_ convert undefined to null.
    /// It is viewing the type of a value in-place, before it is retrieved.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // only references are implemented for `Path`
    pub fn at_path<'a>(&self, path: impl ValuePath<'a>) -> Self {
        self.get_recursive(path.segment_iter())
    }

    fn get_field(&self, field: Cow<'_, str>) -> Self {
        self.as_object().map_or_else(Self::undefined, |object| {
            let mut kind = object
                .known()
                .get(&field.into_owned().into())
                .cloned()
                .unwrap_or_else(|| object.unknown_kind());

            if !self.is_exact() {
                kind = kind.or_undefined();
            }
            kind
        })
    }

    fn get_recursive<'a>(
        &self,
        mut iter: impl Iterator<Item = BorrowedSegment<'a>> + Clone,
    ) -> Self {
        if self.is_never() {
            // a terminating expression by definition can "never" resolve to a value
            return Self::never();
        }

        match iter.next() {
            Some(BorrowedSegment::Field(field) | BorrowedSegment::CoalesceEnd(field)) => {
                self.get_field(field).get_recursive(iter)
            }
            Some(BorrowedSegment::Index(mut index)) => {
                if let Some(array) = self.as_array() {
                    if index < 0 {
                        let largest_known_index = array.known().keys().map(|i| i.to_usize()).max();
                        // The minimum size of the resulting array.
                        let len_required = -index as usize;

                        if array.unknown_kind().contains_any_defined() {
                            // The exact length is not known. We can't know for sure if the index
                            // will point to a known or unknown type, so the union of the unknown type
                            // plus any possible known type must be taken. Just the unknown type alone is not sufficient.

                            // The array may be larger, but this is the largest we can prove the array is from the type information.
                            let min_length = largest_known_index.map_or(0, |i| i + 1);

                            // We can prove the positive index won't be less than "min_index".
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
                        }

                        // There are no unknown indices, so we can determine the exact positive index.
                        let exact_len = largest_known_index.map_or(0, |x| x + 1);
                        if exact_len >= len_required {
                            // Make the index positive, then continue below.
                            index += exact_len as isize;
                        } else {
                            // Out of bounds index.
                            return Self::undefined();
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
                    Self::undefined()
                }
            }
            Some(BorrowedSegment::CoalesceField(field)) => {
                let field_kind = self.get_field(field);

                // The remaining segments if this match succeeded.
                #[allow(clippy::needless_collect)]
                // Need to collect to prevent infinite recursive iterator type.
                let match_iter = iter
                    .clone()
                    .skip_while(|segment| matches!(segment, BorrowedSegment::CoalesceField(_)))
                    // Skip the `CoalesceEnd`, which always exists after `CoalesceFields`.
                    .skip(1)
                    .collect::<Vec<_>>();

                // This is the resulting type, assuming the match succeeded.
                let match_type = field_kind
                    // This type is only valid if the match succeeded, which means this type wasn't undefined.
                    .without_undefined()
                    .get_recursive(match_iter.into_iter());

                if !field_kind.contains_undefined() {
                    // This coalesce field will always be defined, so skip the others.
                    return match_type;
                }

                // The first field may or may not succeed. Try both.
                self.get_recursive(iter).union(match_type)
            }
            Some(BorrowedSegment::Invalid) => {
                // Value::get returns `None` in this case, which means the value is not defined.
                Self::undefined()
            }
            None => self.clone(),
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
    fn test_at_path() {
        struct TestCase {
            kind: Kind,
            path: OwnedValuePath,
            want: Kind,
        }

        for (title, TestCase { kind, path, want }) in [
            (
                "get root",
                TestCase {
                    kind: Kind::bytes(),
                    path: owned_value_path!(),
                    want: Kind::bytes(),
                },
            ),
            (
                "get field from non-object",
                TestCase {
                    kind: Kind::bytes(),
                    path: owned_value_path!("foo"),
                    want: Kind::undefined(),
                },
            ),
            (
                "get field from object",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                    path: owned_value_path!("a"),
                    want: Kind::integer(),
                },
            ),
            (
                "get field from maybe an object",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])).or_null(),
                    path: owned_value_path!("a"),
                    want: Kind::integer().or_undefined(),
                },
            ),
            (
                "get unknown from object with no unknown",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                    path: owned_value_path!("b"),
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
                    path: owned_value_path!("b"),
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
                    path: owned_value_path!("b"),
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
                    path: owned_value_path!("a", "b"),
                    want: Kind::integer(),
                },
            ),
            (
                "get index from non-array",
                TestCase {
                    kind: Kind::bytes(),
                    path: owned_value_path!(1),
                    want: Kind::undefined(),
                },
            ),
            (
                "get index from array",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                    path: owned_value_path!(0),
                    want: Kind::integer(),
                },
            ),
            (
                "get index from maybe array",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])).or_bytes(),
                    path: owned_value_path!(0),
                    want: Kind::integer().or_undefined(),
                },
            ),
            (
                "get unknown from array with no unknown",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                    path: owned_value_path!(1),
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
                    path: owned_value_path!(1),
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
                    path: owned_value_path!(1),
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
                    path: owned_value_path!(0, 0),
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
                    path: owned_value_path!(-2),
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
                    path: owned_value_path!(-1),
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
                    path: owned_value_path!(-2),
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
                    path: owned_value_path!(-2),
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
                    path: owned_value_path!(-2),
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
                    path: owned_value_path!(-2, "foo"),
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
                    path: parse_value_path(".(a|b)").unwrap(),
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
                    path: parse_value_path(".(a|b)").unwrap(),
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
                    path: parse_value_path(".(a|b)").unwrap(),
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
                    path: parse_value_path(".(a|b)").unwrap(),
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
                    path: parse_value_path(".(a|b).foo").unwrap(),
                    want: Kind::bytes().or_undefined(),
                },
            ),
            (
                "nested terminating expression",
                TestCase {
                    kind: Kind::never(),
                    path: owned_value_path!(".foo.bar"),
                    want: Kind::never(),
                },
            ),
        ] {
            assert_eq!(kind.at_path(&path), want, "test: {title}");
        }
    }
}
