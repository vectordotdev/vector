use crate::kind::collection::EmptyState;
use crate::kind::{Collection, Field};
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

        // match (first, second) {
        //     (OwnedSegment::Field())
        // }

        if let Some(second) = second {
            // unimplemented!("more segments");
            match first {
                OwnedSegment::Field(field) => {
                    if let Some(object) = self.as_object_mut() {
                        match object.known_mut().get_mut(&Field::from(field.to_owned())) {
                            None => {
                                unimplemented!()
                            }
                            Some(child) => {
                                let compact_options = child.remove_inner(&segments[1..], compact);

                                if compact_options.should_compact
                                    && !compact_options.should_not_compact
                                {
                                    // always compact
                                    object.known_mut().remove(&Field::from(field.to_owned()));
                                } else if compact_options.should_compact
                                    && compact_options.should_not_compact
                                {
                                    // maybe compact
                                    let not_compacted = object.clone();
                                    object.known_mut().remove(&Field::from(field.to_owned()));
                                    object.merge(not_compacted, false);
                                } else {
                                    // never compact: do nothing, already correct.
                                }

                                // The compaction is propagated only if the current collection is also empty
                                compact_options.disable_should_compact(
                                    !CompactOptions::from(object.is_empty()).should_compact,
                                )
                            }
                        }
                        // let removed_known = object
                        //     .known_mut()
                        //     .remove(&field.as_str().into())
                        //     .map_or(false, |kind| kind.contains_any_defined());
                        //
                        // let maybe_removed_unknown = object.unknown_kind().contains_any_defined();
                        //
                        // CompactOptions::from(object.is_empty())
                        //     .disable_should_compact(!removed_known && !maybe_removed_unknown)
                    } else {
                        unimplemented!()
                    }
                }
                OwnedSegment::Index(_) => unimplemented!(),
                OwnedSegment::Coalesce(_) => unimplemented!(),
                OwnedSegment::Invalid => unimplemented!(),
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
                        unimplemented!()
                    }
                }
                OwnedSegment::Index(_) => unimplemented!(),
                OwnedSegment::Coalesce(_) => unimplemented!(),
                OwnedSegment::Invalid => unimplemented!(),
            }
        }

        // match (first, second) {
        //     (_, None) => {
        //
        //     }
        //     _ => unimplemented!()
        // }

        // match segments.first().expect("must not be empty") {
        //     OwnedSegment::Field(_) => unimplemented!(),
        //     OwnedSegment::Index(_) => unimplemented!(),
        //     OwnedSegment::Coalesce(_) => unimplemented!(),
        //     OwnedSegment::Invalid => {
        //         // Same behavior as `Value::remove`, do nothing. Eventually
        //         // this variant should be removed from `OwnedValuePath`.
        //         return;
        //     }
        // }
    }
}

#[derive(Debug)]
struct CompactOptions {
    should_compact: bool,
    should_not_compact: bool,
}

impl CompactOptions {
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
            // (
            //     "remove nested object field: maybe compact",
            //     TestCase {
            //         kind: Kind::object(Collection::from(BTreeMap::from([(
            //             "a".into(),
            //             Kind::object(Collection::empty().with_unknown(Kind::float())),
            //         )]))),
            //         path: owned_value_path!("a", "b"),
            //         compact: true,
            //         want: Kind::object(Collection::empty()),
            //     },
            // ),
            // (
            //     "remove nested object field: compact",
            //     TestCase {
            //         kind: Kind::object(Collection::from(BTreeMap::from([(
            //             "a".into(),
            //             Kind::object(Collection::from(BTreeMap::from([(
            //                 "b".into(),
            //                 Kind::integer(),
            //             )]))),
            //         )]))),
            //         path: owned_value_path!("a", "b"),
            //         compact: true,
            //         want: Kind::object(Collection::empty()),
            //     },
            // ),
        ] {
            println!("Test: {:?}", title);
            let mut actual = kind;
            actual.remove(&path, compact);
            if actual != want {
                panic!(
                    "Test failed: {:?}.\nExpected = {:?}\nActual = {:?}",
                    title,
                    want.debug_info(),
                    actual.debug_info()
                );
            }
        }
    }
}
