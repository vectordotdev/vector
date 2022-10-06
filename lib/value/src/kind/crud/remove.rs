use crate::kind::collection::{CollectionRemove, EmptyState};
use crate::kind::{Collection, Field, Index};
use crate::Kind;
use lookup::lookup_v2::OwnedSegment;
use lookup::OwnedValuePath;

impl Kind {
    /// Removes the `Kind` at the given `path` within `self`.
    /// This has the same behavior as `Value::remove`.
    #[allow(clippy::needless_pass_by_value)] // only reference types implement Path
    pub fn remove(&mut self, path: &OwnedValuePath, prune: bool) {
        // TODO: add return type which is just `Kind::get` of the query

        let mut segments = &path.segments;
        if segments.is_empty() {
            let mut new_kind = Kind::never();
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
    }

    fn remove_inner(&mut self, segments: &[OwnedSegment], compact: bool) -> CompactOptions {
        debug_assert!(!segments.is_empty());

        let first = segments.first().expect("must not be empty");
        let second = segments.get(1);

        if let Some(second) = second {
            // more than 1 segment left

            match first {
                OwnedSegment::Field(field) => {
                    if let Some(object) = self.as_object_mut() {
                        match object.known_mut().get_mut(&Field::from(field.to_owned())) {
                            None => CompactOptions {
                                should_compact: object
                                    .unknown_kind()
                                    .at_path(&segments[1..])
                                    .contains_any_defined(),
                                should_not_compact: true,
                            }
                            .apply_global_compact_option(compact),
                            Some(child) => child
                                .remove_inner(&segments[1..], compact)
                                .compact(object, field.to_owned()),
                        }
                    } else {
                        // guaranteed to not delete anything
                        CompactOptions {
                            should_compact: false,
                            should_not_compact: true,
                        }
                    }
                }
                OwnedSegment::Index(index) => {
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
                                        match single_remove.known_mut().get_mut(&i.into()) {
                                            None => {
                                                unimplemented!()
                                                // CompactOptions {
                                                //     should_compact: array
                                                //         .unknown_kind()
                                                //         .at_path(&segments[1..])
                                                //         .contains_any_defined(),
                                                //     should_not_compact: true,
                                                // }
                                                //     .apply_global_compact_option(compact),
                                            }
                                            Some(child) => {
                                                child
                                                    .remove_inner(&segments[1..], compact)
                                                    .compact(&mut single_remove, i);
                                            }
                                        }
                                        array.merge(single_remove, false);
                                    }
                                }
                                return CompactOptions {
                                    // TODO: add more conditions here (index 0 might not be removed)
                                    should_compact: min_index == 0,
                                    should_not_compact: true,
                                };
                            } else {
                                if let Some(positive_index) = array.get_positive_index(index) {
                                    index = positive_index as isize;
                                } else {
                                    // Removing a non-existing index
                                    return CompactOptions::from(EmptyState::NeverEmpty);
                                }
                            }
                        }

                        match array.known_mut().get_mut(&(index as usize).into()) {
                            None => CompactOptions {
                                should_compact: array
                                    .unknown_kind()
                                    .at_path(&segments[1..])
                                    .contains_any_defined(),
                                should_not_compact: true,
                            }
                            .apply_global_compact_option(compact),
                            Some(child) => child
                                .remove_inner(&segments[1..], compact)
                                .compact(array, index as usize),
                        }
                    } else {
                        // guaranteed to not delete anything
                        CompactOptions {
                            should_compact: false,
                            should_not_compact: true,
                        }
                    }
                }
                OwnedSegment::Coalesce(_) => unimplemented!(),
                OwnedSegment::Invalid => {
                    // guaranteed to not delete anything
                    CompactOptions {
                        should_compact: false,
                        should_not_compact: true,
                    }
                }
            }
        } else {
            match first {
                OwnedSegment::Field(field) => {
                    if let Some(object) = self.as_object_mut() {
                        let removed_known = object
                            .known_mut()
                            .remove(&field.as_str().into())
                            .map_or(false, |kind| kind.contains_any_defined());

                        let maybe_removed_unknown = object.unknown_kind().contains_any_defined();

                        CompactOptions::from(object.is_empty())
                            .disable_should_compact(!removed_known && !maybe_removed_unknown)
                            .apply_global_compact_option(compact)
                    } else {
                        CompactOptions::from(EmptyState::NeverEmpty)
                    }
                }
                OwnedSegment::Index(index) => {
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
                                        let mut single_shift = original.clone();
                                        single_shift.remove_shift(i);
                                        array.merge(single_shift, false);
                                    }
                                }
                                return CompactOptions {
                                    should_compact: min_index == 0,
                                    should_not_compact: true,
                                };
                            } else {
                                if let Some(positive_index) = array.get_positive_index(index) {
                                    index = positive_index as isize;
                                } else {
                                    // Removing a non-existing index
                                    return CompactOptions::from(EmptyState::NeverEmpty);
                                }
                            }
                        }

                        let index = index as usize;
                        let min_length = array.min_length();
                        if min_length >= index {
                            array.remove_shift(index);
                            CompactOptions::from(array.is_empty())
                                .apply_global_compact_option(compact)
                        } else {
                            // The removed index isn't guaranteed to exist.
                            // Elements might be removed, but known indices won't be modified.
                            // Unknown values don't change from a removal, so there is nothing to change here.
                            CompactOptions::from(EmptyState::MaybeEmpty)
                                .apply_global_compact_option(compact)
                        }
                    } else {
                        CompactOptions::from(EmptyState::NeverEmpty)
                    }
                }
                OwnedSegment::Coalesce(_) => unimplemented!(),
                OwnedSegment::Invalid => unimplemented!(),
            }
        }
    }
}

#[derive(Debug)]
struct CompactOptions {
    should_compact: bool,
    should_not_compact: bool,
}

impl CompactOptions {
    fn compact<T>(self, collection: &mut Collection<T>, key: impl Into<T>) -> Self
    where
        T: Ord + Clone,
        Collection<T>: CollectionRemove<Key = T>,
    {
        let key = &key.into();
        if self.should_compact && !self.should_not_compact {
            // always compact
            collection.remove_known(key);
        } else if self.should_compact && self.should_not_compact {
            // maybe compact
            let not_compacted = collection.clone();
            collection.remove_known(key);
            collection.merge(not_compacted, false);
        } else {
            // never compact: do nothing, already correct.
        }

        // The compaction is propagated only if the current collection is also empty
        self.disable_should_compact(!CompactOptions::from(collection.is_empty()).should_compact)
    }

    /// If the value is true, the `should_compact` option is set to false
    fn disable_should_compact(mut self, value: bool) -> Self {
        if value {
            self.should_compact = false;
        }
        self
    }

    /// If the global "compact" option is set to false, compaction is disabled
    fn apply_global_compact_option(mut self, value: bool) -> Self {
        if value {
            self
        } else {
            Self {
                should_compact: false,
                should_not_compact: true,
            }
        }
    }
}

impl From<EmptyState> for CompactOptions {
    fn from(state: EmptyState) -> Self {
        match state {
            EmptyState::NeverEmpty => Self {
                should_compact: false,
                should_not_compact: true,
            },
            EmptyState::MaybeEmpty => Self {
                should_compact: true,
                should_not_compact: true,
            },
            EmptyState::AlwaysEmpty => Self {
                should_compact: true,
                should_not_compact: false,
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
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
        }

        for (
            title,
            TestCase {
                kind,
                path,
                compact,
                want,
            },
        ) in [
            (
                "remove integer root",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!(),
                    compact: false,
                    want: Kind::null(),
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
                },
            ),
            (
                "remove unknown object field 2",
                TestCase {
                    kind: Kind::object(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_value_path!("a", "b"),
                    compact: false,
                    want: Kind::object(Collection::empty().with_unknown(Kind::integer())),
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
                },
            ),
            (
                "remove field from non object",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!("a", "b", "c"),
                    compact: true,
                    want: Kind::integer(),
                },
            ),
            (
                "remove index from non array",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!(1, 2, 3),
                    compact: true,
                    want: Kind::integer(),
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
                },
            ),
            (
                "remove field from non-object",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!("a"),
                    compact: false,
                    want: Kind::integer(),
                },
            ),
            (
                "remove index from non-array",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!(0),
                    compact: false,
                    want: Kind::integer(),
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
                },
            ),
            (
                "remove negative index empty array",
                TestCase {
                    kind: Kind::array(Collection::empty()),
                    path: owned_value_path!(-1),
                    compact: false,
                    want: Kind::array(Collection::empty()),
                },
            ),
            (
                "remove negative index with unknown",
                TestCase {
                    kind: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_value_path!(-1),
                    compact: false,
                    want: Kind::array(Collection::empty().with_unknown(Kind::integer())),
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
                    want: Kind::array(Collection::from(BTreeMap::from([(
                        0.into(),
                        Kind::bytes(),
                    )]))),
                },
            ),
        ] {
            println!("Test: {:?}", title);
            let mut actual = kind;
            actual.remove(&path, compact);
            if actual != want {
                panic!(
                    "Test failed: {:#?}.\nExpected = {:#?}\nActual =   {:#?}",
                    title,
                    want.debug_info(),
                    actual.debug_info()
                );
            }
        }
    }
}
