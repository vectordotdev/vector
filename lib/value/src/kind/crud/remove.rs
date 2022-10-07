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
            println!("Input = {:?}", self.debug_info());
            let _compact_options = self.remove_inner(segments, prune);
        }
    }

    fn remove_inner(&mut self, segments: &[OwnedSegment], compact: bool) -> CompactOptions {
        if self.is_never() {
            // If `self` is `never`, the program would have already terminated
            // so this removal can't happen.
            *self = Self::never();
            return CompactOptions {
                should_compact: false,
                should_not_compact: true,
            };
        }

        if let Some(first) = segments.first() {
        } else {
            return CompactOptions {
                should_compact: self.contains_any_defined(),
                should_not_compact: self.contains_undefined(),
            };
        }

        match segments.first().expect("must not be empty") {
            OwnedSegment::Field(field) => {
                let mut at_path_kind = self.at_path(segments);
                if let Some(object) = self.as_object_mut() {
                    match object.known_mut().get_mut(&Field::from(field.to_owned())) {
                        None => {
                            // The modified value is discarded here (It's not needed)
                            &mut at_path_kind
                        }
                        Some(child) => child,
                    }
                    .remove_inner(&segments[1..], compact)
                    .compact(object, field.to_owned(), compact)
                } else {
                    // guaranteed to not delete anything
                    CompactOptions {
                        should_compact: false,
                        should_not_compact: true,
                    }
                }
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

                            return CompactOptions {
                                should_compact: array.min_length() <= 1,
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
                        None => {
                            // The modified value is discarded here (It's not needed)
                            &mut at_path_kind
                        }
                        Some(child) => child,
                    }
                    .remove_inner(&segments[1..], compact)
                    .compact(array, index as usize, compact)
                } else {
                    // guaranteed to not delete anything
                    CompactOptions {
                        should_compact: false,
                        should_not_compact: true,
                    }
                }
            }
            OwnedSegment::Coalesce(fields) => {
                let original = self.clone();
                if let Some(object) = self.as_object_mut() {
                    let mut output = Kind::never();

                    let mut compact_options = CompactOptions {
                        should_compact: false,
                        should_not_compact: false,
                    };

                    for field in fields {
                        let field_kind =
                            original.at_path(&[OwnedSegment::Field(field.to_string())]);

                        if field_kind.contains_any_defined() {
                            let mut child_kind = original.clone();
                            let mut child_segments = segments.to_vec();
                            child_segments[0] = OwnedSegment::Field(field.to_owned());

                            compact_options.merge(
                                child_kind.remove_inner(&child_segments, compact).compact(
                                    object,
                                    field.to_string(),
                                    compact,
                                ),
                            );

                            output = output.union(child_kind);

                            if !field_kind.contains_undefined() {
                                // No other field will be visited, so return early
                                break;
                            }
                        }
                    }
                    *self = output;
                    compact_options
                } else {
                    // guaranteed to not delete anything
                    CompactOptions {
                        should_compact: false,
                        should_not_compact: true,
                    }
                }
            }
            OwnedSegment::Invalid => {
                // guaranteed to not delete anything
                CompactOptions {
                    should_compact: false,
                    should_not_compact: true,
                }
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

        CompactOptions::from(collection.is_empty())
            .disable_should_compact(!self.should_compact)
            .disable_should_compact(!continue_compact)
    }

    /// Combines two sets of options. Each setting is "or"ed together.
    fn merge(&mut self, other: Self) {
        self.should_compact |= other.should_compact;
        self.should_not_compact |= other.should_not_compact;
    }

    /// If the value is true, the `should_compact` option is set to false
    fn disable_should_compact(mut self, value: bool) -> Self {
        if value {
            self.should_compact = false;
            self.should_not_compact = true;
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
                },
            ),
            (
                "remove nested negative unknown index - empty array",
                TestCase {
                    kind: Kind::array(Collection::empty().with_unknown(Kind::float())),
                    path: owned_value_path!(-1, 0),
                    compact: false,
                    want: Kind::array(Collection::empty().with_unknown(Kind::float())),
                },
            ),
            (
                "coalesce 1",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_value_path("(a|b)"),
                    compact: false,
                    want: Kind::object(Collection::empty()),
                },
            ),
            (
                "coalesce 2",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([(
                        "b".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_value_path("(a|b)"),
                    compact: false,
                    want: Kind::object(Collection::empty()),
                },
            ),
            (
                "coalesce 3",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_undefined()),
                        ("b".into(), Kind::float()),
                    ]))),
                    path: parse_value_path("(a|b)"),
                    compact: false,
                    want: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_undefined()),
                        ("b".into(), Kind::float().or_undefined()),
                    ]))),
                },
            ),
            (
                "coalesce 4",
                TestCase {
                    kind: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_undefined()),
                        ("b".into(), Kind::float().or_undefined()),
                    ]))),
                    path: parse_value_path("(a|b)"),
                    compact: false,
                    want: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_undefined()),
                        ("b".into(), Kind::float().or_undefined()),
                    ]))),
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
                    path: parse_value_path("(a|a2).b"),
                    compact: false,
                    want: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::object(Collection::empty()),
                    )]))),
                },
            ),
        ] {
            println!("=========== Test: {:?} ===========", title);
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
