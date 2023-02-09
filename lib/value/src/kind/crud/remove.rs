//! All code related to removing from a [`Kind`].

use crate::kind::collection::{CollectionRemove, EmptyState};
use crate::kind::{Collection, Field};
use crate::Kind;
use lookup::lookup_v2::OwnedSegment;
use lookup::OwnedValuePath;

impl Kind {
    /// Removes the `Kind` at the given `path` within `self`.
    /// This has the same behavior as `Value::remove`.
    #[allow(clippy::return_self_not_must_use)] // It is fine to ignore the output here
    pub fn remove(&mut self, path: &OwnedValuePath, prune: bool) -> Self {
        let removed_type = self.get(path);

        let segments = &path.segments;
        if segments.is_empty() {
            let mut new_kind = Self::never();
            if self.contains_object() {
                new_kind.add_object(Collection::empty());
            }
            if self.contains_array() {
                new_kind.add_array(Collection::empty());
            }
            if self.contains_primitive() {
                // non-collection types are set to null when deleted at the root
                new_kind.add_null();
            }
            *self = new_kind;
        } else {
            let _compact_options = self.remove_inner(segments, prune);
        }
        removed_type
    }

    #[allow(clippy::too_many_lines)]
    fn remove_inner(&mut self, segments: &[OwnedSegment], compact: bool) -> CompactOptions {
        if self.is_never() {
            // If `self` is `never`, the program would have already terminated
            // so this removal can't happen.
            *self = Self::never();
            return CompactOptions::Never;
        }

        if let Some(first) = segments.first() {
            match first {
                OwnedSegment::Field(field) => {
                    let mut at_path_kind = self.at_path(segments);

                    self.as_object_mut()
                        .map_or(CompactOptions::Never, |object| {
                            // The modified value is discarded here (It's not needed)
                            object
                                .known_mut()
                                .get_mut(&Field::from(field.clone()))
                                .unwrap_or(&mut at_path_kind)
                                .remove_inner(&segments[1..], compact)
                                .compact(object, field.clone(), compact)
                        })
                }

                OwnedSegment::Index(index) => {
                    let mut at_path_kind = self.at_path(segments);
                    if let Some(array) = self.as_array_mut() {
                        let mut index = *index;
                        if index < 0 {
                            let negative_index = (-index) as usize;

                            if array.unknown_kind().contains_any_defined() {
                                let original = array.clone();
                                *array = original.clone();

                                let min_index = array
                                    .largest_known_index()
                                    .map_or(0, |x| x + 1 - negative_index);

                                if let Some(largest_known_index) = array.largest_known_index() {
                                    for i in min_index..=largest_known_index {
                                        let mut single_remove = original.clone();
                                        if let Some(child) =
                                            single_remove.known_mut().get_mut(&i.into())
                                        {
                                            child.remove_inner(&segments[1..], compact).compact(
                                                &mut single_remove,
                                                i,
                                                compact,
                                            );
                                        }
                                        array.merge(single_remove, false);
                                    }
                                }
                                return if array.min_length() <= 1 {
                                    CompactOptions::Maybe
                                } else {
                                    CompactOptions::Never
                                };
                            } else if let Some(positive_index) = array.get_positive_index(index) {
                                index = positive_index as isize;
                            } else {
                                // Removing a non-existing index
                                return CompactOptions::from(EmptyState::Never);
                            }
                        }

                        // The modified value is discarded here (It's not needed)
                        array
                            .known_mut()
                            .get_mut(&(index as usize).into())
                            .unwrap_or(&mut at_path_kind)
                            .remove_inner(&segments[1..], compact)
                            .compact(array, index as usize, compact)
                    } else {
                        // guaranteed to not delete anything
                        CompactOptions::Never
                    }
                }
                OwnedSegment::Coalesce(fields) => {
                    let original = self.clone();

                    #[allow(clippy::option_if_let_else)] // false positive due to lifetime issues
                    if let Some(object) = self.as_object_mut() {
                        let mut output = Self::never();

                        let mut compact_options = None;

                        for field in fields {
                            let field_kind =
                                original.at_path([OwnedSegment::Field(field.to_string())].as_ref());

                            if field_kind.contains_any_defined() {
                                let mut child_kind = original.clone();
                                let mut child_segments = segments.to_vec();
                                child_segments[0] = OwnedSegment::Field(field.clone());

                                let child_compact_options = child_kind
                                    .remove_inner(&child_segments, compact)
                                    .compact(object, field.to_string(), compact);

                                compact_options = Some(compact_options.map_or(
                                    child_compact_options,
                                    |compact_options: CompactOptions| {
                                        compact_options.or(child_compact_options)
                                    },
                                ));
                                output = output.union(child_kind);

                                if !field_kind.contains_undefined() {
                                    // No other field will be visited, so return early
                                    break;
                                }
                            }
                        }
                        *self = output;
                        compact_options.unwrap_or(CompactOptions::Never)
                    } else {
                        // guaranteed to not delete anything
                        CompactOptions::Never
                    }
                }
            }
        } else {
            CompactOptions::new(self.contains_any_defined(), self.contains_undefined())
        }
    }
}

/// A type definition might not know for sure if compaction will occur or not, so this
/// keeps track of the current state
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CompactOptions {
    /// Compaction will always happen.
    Always,
    /// Compaction may or may not happen. Both possibilities should be merged together.
    Maybe,
    /// Compaction will never happen.
    Never,
}

impl CompactOptions {
    fn new(compact: bool, dont_compact: bool) -> Self {
        match (compact, dont_compact) {
            (true, false) => Self::Always,
            (false, true) => Self::Never,
            (true, true) => Self::Maybe,
            (false, false) => unreachable!("Invalid CompactOptions"),
        }
    }

    fn compact<T>(
        self,
        collection: &mut Collection<T>,
        key: impl Into<T>,
        continue_compact: bool,
    ) -> Self
    where
        T: Ord + Clone + std::fmt::Debug,
        Collection<T>: CollectionRemove<Key = T>,
    {
        let key = &key.into();

        match self {
            Self::Always => collection.remove_known(key),
            Self::Maybe => {
                let not_compacted = collection.clone();
                collection.remove_known(key);
                collection.merge(not_compacted, false);
            }
            Self::Never => {
                // do nothing, already correct}
            }
        }

        Self::from(collection.is_empty())
            .disable_should_compact(!self.should_compact())
            .disable_should_compact(!continue_compact)
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn should_compact(&self) -> bool {
        match self {
            Self::Always | Self::Maybe => true,
            Self::Never => false,
        }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn should_not_compact(&self) -> bool {
        match self {
            Self::Always => false,
            Self::Maybe | Self::Never => true,
        }
    }

    /// Combines two sets of options. Each setting is "or"ed together.
    fn or(self, other: Self) -> Self {
        Self::new(
            self.should_compact() || other.should_compact(),
            self.should_not_compact() || other.should_not_compact(),
        )
    }

    /// If the value is true, the `should_compact` option is set to false.
    fn disable_should_compact(self, value: bool) -> Self {
        if value {
            Self::Never
        } else {
            self
        }
    }
}

impl From<EmptyState> for CompactOptions {
    fn from(state: EmptyState) -> Self {
        match state {
            EmptyState::Never => Self::Never,
            EmptyState::Maybe => Self::Maybe,
            EmptyState::Always => Self::Always,
        }
    }
}

#[cfg(test)]
#[allow(clippy::manual_assert)]
mod test {
    use super::*;
    use lookup::lookup_v2::parse_value_path;
    use lookup::owned_value_path;
    use std::collections::BTreeMap;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_remove() {
        struct TestCase {
            kind: Kind,
            path: OwnedValuePath,
            compact: bool,
            want: Kind,
            return_value: Kind,
        }

        for (
            title,
            TestCase {
                kind,
                path,
                compact,
                want,
                return_value,
            },
        ) in [
            (
                "remove integer root",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!(),
                    compact: false,
                    want: Kind::null(),
                    return_value: Kind::integer(),
                },
            ),
            (
                "remove array root",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::integer(),
                    )]))),
                    path: owned_value_path!(),
                    compact: false,
                    want: Kind::array(Collection::empty()),
                    return_value: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::integer(),
                    )]))),
                },
            ),
            (
                "remove object root",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::integer(),
                    )]))),
                    path: owned_value_path!(),
                    compact: false,
                    want: Kind::object(Collection::empty()),
                    return_value: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::integer(),
                    )]))),
                },
            ),
            (
                "remove object field",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::integer(),
                    )]))),
                    path: owned_value_path!("a"),
                    compact: false,
                    want: Kind::object(Collection::empty()),
                    return_value: Kind::integer(),
                },
            ),
            (
                "remove nested object field",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::from(BTreeMap::from([(
                            "b".into(),
                            Kind::integer(),
                        )]))),
                    )]))),
                    path: owned_value_path!("a", "b"),
                    compact: false,
                    want: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::empty()),
                    )]))),
                    return_value: Kind::integer(),
                },
            ),
            (
                "remove nested object field: compact",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::from(BTreeMap::from([(
                            "b".into(),
                            Kind::integer(),
                        )]))),
                    )]))),
                    path: owned_value_path!("a", "b"),
                    compact: true,
                    want: Kind::object(Collection::empty()),
                    return_value: Kind::integer(),
                },
            ),
            (
                "remove object unknown field",
                TestCase {
                    kind: Kind::object(
                        Collection::from(BTreeMap::from([("a".into(), Kind::integer())]))
                            .with_unknown(Kind::float()),
                    ),
                    path: owned_value_path!("b"),
                    compact: false,
                    want: Kind::object(
                        Collection::from(BTreeMap::from([("a".into(), Kind::integer())]))
                            .with_unknown(Kind::float()),
                    ),
                    return_value: Kind::float().or_null(),
                },
            ),
            (
                "remove object unknown field: compact",
                TestCase {
                    kind: Kind::object(
                        Collection::from(BTreeMap::from([("a".into(), Kind::integer())]))
                            .with_unknown(Kind::float()),
                    ),
                    path: owned_value_path!("b"),
                    compact: true,
                    want: Kind::object(
                        Collection::from(BTreeMap::from([("a".into(), Kind::integer())]))
                            .with_unknown(Kind::float()),
                    ),
                    return_value: Kind::float().or_null(),
                },
            ),
            (
                "remove nested object field: maybe compact",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::empty().with_unknown(Kind::float())),
                    )]))),
                    path: owned_value_path!("a", "b"),
                    compact: true,
                    want: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::empty().with_unknown(Kind::float()))
                            .or_undefined(),
                    )]))),
                    return_value: Kind::float().or_null(),
                },
            ),
            (
                "remove unknown object field 2",
                TestCase {
                    kind: Kind::object(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_value_path!("a", "b"),
                    compact: false,
                    want: Kind::object(Collection::empty().with_unknown(Kind::integer())),
                    return_value: Kind::null(),
                },
            ),
            (
                "remove deep nested unknown object field",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::empty().with_unknown(Kind::any_object())),
                    )]))),
                    path: owned_value_path!("a", "b", "c"),
                    compact: false,
                    want: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::empty().with_unknown(Kind::any_object())),
                    )]))),
                    return_value: Kind::any().without_undefined(),
                },
            ),
            (
                "remove deep nested unknown object field: compact",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::empty().with_unknown(Kind::any_object())),
                    )]))),
                    path: owned_value_path!("a", "b", "c"),
                    compact: true,
                    want: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::empty().with_unknown(Kind::any_object()))
                            .or_undefined(),
                    )]))),
                    return_value: Kind::any().without_undefined(),
                },
            ),
            (
                "remove field from non object",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!("a", "b", "c"),
                    compact: true,
                    want: Kind::integer(),
                    return_value: Kind::null(),
                },
            ),
            (
                "remove index from non array",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!(1, 2, 3),
                    compact: true,
                    want: Kind::integer(),
                    return_value: Kind::null(),
                },
            ),
            (
                "remove known index 0",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::integer(),
                    )]))),
                    path: owned_value_path!(0),
                    compact: true,
                    want: Kind::array(Collection::empty()),
                    return_value: Kind::integer(),
                },
            ),
            (
                "remove known index 0, shift elements",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([
                        (0.into(), Kind::integer()),
                        (1.into(), Kind::float()),
                    ]))),
                    path: owned_value_path!(0),
                    compact: true,
                    want: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::float(),
                    )]))),
                    return_value: Kind::integer(),
                },
            ),
            (
                "remove known index 1, shift elements",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([
                        (0.into(), Kind::integer()),
                        (1.into(), Kind::float()),
                        (2.into(), Kind::bytes()),
                    ]))),
                    path: owned_value_path!(1),
                    compact: true,
                    want: Kind::array(Collection::from(BTreeMap::from([
                        (0.into(), Kind::integer()),
                        (1.into(), Kind::bytes()),
                    ]))),
                    return_value: Kind::float(),
                },
            ),
            (
                "remove field from non-object",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!("a"),
                    compact: false,
                    want: Kind::integer(),
                    return_value: Kind::null(),
                },
            ),
            (
                "remove index from non-array",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!(0),
                    compact: false,
                    want: Kind::integer(),
                    return_value: Kind::null(),
                },
            ),
            (
                "remove index -1",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::integer(),
                    )]))),
                    path: owned_value_path!(-1),
                    compact: false,
                    want: Kind::array(Collection::empty()),
                    return_value: Kind::integer(),
                },
            ),
            (
                "remove index -2",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([
                        (0.into(), Kind::integer()),
                        (1.into(), Kind::float()),
                    ]))),
                    path: owned_value_path!(-2),
                    compact: false,
                    want: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::float(),
                    )]))),
                    return_value: Kind::integer(),
                },
            ),
            (
                "remove negative index non-existing element",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::integer(),
                    )]))),
                    path: owned_value_path!(-2),
                    compact: false,
                    want: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::integer(),
                    )]))),
                    return_value: Kind::null(),
                },
            ),
            (
                "remove negative index empty array",
                TestCase {
                    kind: Kind::array(Collection::empty()),
                    path: owned_value_path!(-1),
                    compact: false,
                    want: Kind::array(Collection::empty()),
                    return_value: Kind::null(),
                },
            ),
            (
                "remove negative index with unknown",
                TestCase {
                    kind: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_value_path!(-1),
                    compact: false,
                    want: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    return_value: Kind::integer().or_null(),
                },
            ),
            (
                "remove negative index with unknown 2",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::float())]))
                            .with_unknown(Kind::integer()),
                    ),
                    path: owned_value_path!(-1),
                    compact: false,
                    want: Kind::array(
                        Collection::from(BTreeMap::from([(
                            0.into(),
                            Kind::float().or_integer().or_undefined(),
                        )]))
                        .with_unknown(Kind::integer()),
                    ),
                    return_value: Kind::integer().or_float(),
                },
            ),
            (
                "remove negative index with unknown 3",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::float()),
                            (1.into(), Kind::bytes()),
                        ]))
                        .with_unknown(Kind::integer()),
                    ),
                    path: owned_value_path!(-1),
                    compact: false,
                    want: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::float()),
                            (1.into(), Kind::bytes().or_integer().or_undefined()),
                        ]))
                        .with_unknown(Kind::integer()),
                    ),
                    return_value: Kind::integer().or_bytes(),
                },
            ),
            (
                "remove nested index",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([
                        (
                            0.into(),
                            Kind::array(Collection::from(BTreeMap::from([
                                (0.into(), Kind::float()),
                                (1.into(), Kind::integer()),
                            ]))),
                        ),
                        (1.into(), Kind::bytes()),
                    ]))),
                    path: owned_value_path!(0, 0),
                    compact: false,
                    want: Kind::array(Collection::from(BTreeMap::from([
                        (
                            0.into(),
                            Kind::array(Collection::from(BTreeMap::from([(
                                0.into(),
                                Kind::integer(),
                            )]))),
                        ),
                        (1.into(), Kind::bytes()),
                    ]))),
                    return_value: Kind::float(),
                },
            ),
            (
                "remove nested index, compact",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([
                        (
                            0.into(),
                            Kind::array(Collection::from(BTreeMap::from([(
                                0.into(),
                                Kind::float(),
                            )]))),
                        ),
                        (1.into(), Kind::bytes()),
                    ]))),
                    path: owned_value_path!(0, 0),
                    compact: true,
                    want: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::bytes(),
                    )]))),
                    return_value: Kind::float(),
                },
            ),
            (
                "remove nested index, maybe compact",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([
                        (
                            0.into(),
                            Kind::array(
                                Collection::from(BTreeMap::from([(0.into(), Kind::float())]))
                                    .with_unknown(Kind::regex()),
                            ),
                        ),
                        (1.into(), Kind::bytes()),
                    ]))),
                    path: owned_value_path!(0, 0),
                    compact: true,
                    want: Kind::array(Collection::from(BTreeMap::from([
                        (
                            0.into(),
                            Kind::array(Collection::empty().with_unknown(Kind::regex())).or_bytes(),
                        ),
                        (1.into(), Kind::bytes().or_undefined()),
                    ]))),
                    return_value: Kind::float(),
                },
            ),
            (
                "remove nested index, maybe compact",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([
                        (
                            0.into(),
                            Kind::array(Collection::empty().with_unknown(Kind::any())),
                        ),
                        (1.into(), Kind::bytes()),
                    ]))),
                    path: owned_value_path!(0, 0, 0),
                    compact: true,
                    want: Kind::array(Collection::from(BTreeMap::from([
                        (
                            0.into(),
                            Kind::array(Collection::empty().with_unknown(Kind::any())).or_bytes(),
                        ),
                        (1.into(), Kind::bytes().or_undefined()),
                    ]))),
                    return_value: Kind::any().without_undefined(),
                },
            ),
            (
                "remove nested negative index, compact",
                TestCase {
                    kind: Kind::array(Collection::from(BTreeMap::from([
                        (
                            0.into(),
                            Kind::array(Collection::from(BTreeMap::from([(
                                0.into(),
                                Kind::integer(),
                            )]))),
                        ),
                        (1.into(), Kind::bytes()),
                    ]))),
                    path: owned_value_path!(-2, 0),
                    compact: true,
                    want: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::bytes(),
                    )]))),
                    return_value: Kind::integer(),
                },
            ),
            (
                "remove nested negative unknown index",
                TestCase {
                    kind: Kind::array(
                        Collection::from(BTreeMap::from([(
                            0.into(),
                            Kind::array(Collection::from(BTreeMap::from([(
                                0.into(),
                                Kind::integer(),
                            )]))),
                        )]))
                        .with_unknown(Kind::float()),
                    ),
                    path: owned_value_path!(-1, 0),
                    compact: false,
                    want: Kind::array(
                        Collection::from(BTreeMap::from([(
                            0.into(),
                            Kind::array(Collection::from(BTreeMap::from([(
                                0.into(),
                                Kind::integer().or_undefined(),
                            )]))),
                        )]))
                        .with_unknown(Kind::float()),
                    ),
                    return_value: Kind::integer().or_null(),
                },
            ),
            (
                "remove nested negative unknown index - empty array",
                TestCase {
                    kind: Kind::array(Collection::empty().with_unknown(Kind::float())),
                    path: owned_value_path!(-1, 0),
                    compact: false,
                    want: Kind::array(Collection::empty().with_unknown(Kind::float())),
                    return_value: Kind::null(),
                },
            ),
            (
                "coalesce 1",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_value_path("(a|b)").unwrap(),
                    compact: false,
                    want: Kind::object(Collection::empty()),
                    return_value: Kind::integer(),
                },
            ),
            (
                "coalesce 2",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "b".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_value_path("(a|b)").unwrap(),
                    compact: false,
                    want: Kind::object(Collection::empty()),
                    return_value: Kind::integer(),
                },
            ),
            (
                "coalesce 3",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_undefined()),
                        ("b".into(), Kind::float()),
                    ]))),
                    path: parse_value_path("(a|b)").unwrap(),
                    compact: false,
                    want: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_undefined()),
                        ("b".into(), Kind::float().or_undefined()),
                    ]))),
                    return_value: Kind::integer().or_float(),
                },
            ),
            (
                "coalesce 4",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_undefined()),
                        ("b".into(), Kind::float().or_undefined()),
                    ]))),
                    path: parse_value_path("(a|b)").unwrap(),
                    compact: false,
                    want: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_undefined()),
                        ("b".into(), Kind::float().or_undefined()),
                    ]))),
                    return_value: Kind::integer().or_float().or_null(),
                },
            ),
            (
                "nested coalesce 1",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::from(BTreeMap::from([(
                            "b".into(),
                            Kind::integer(),
                        )]))),
                    )]))),
                    path: parse_value_path("(a|a2).b").unwrap(),
                    compact: false,
                    want: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::empty()),
                    )]))),
                    return_value: Kind::integer(),
                },
            ),
        ] {
            let mut actual = kind;
            let actual_return_value = actual.remove(&path, compact);

            if actual != want {
                panic!(
                    "Test failed: {:#?}.\nExpected = {:#?}\nActual =   {:#?}",
                    title,
                    want.debug_info(),
                    actual.debug_info()
                );
            }

            if actual_return_value != return_value {
                panic!(
                    "Test failed - return value: {:#?}.\nExpected = {:#?}\nActual =   {:#?}",
                    title,
                    return_value.debug_info(),
                    actual_return_value.debug_info()
                );
            }
        }
    }
}
