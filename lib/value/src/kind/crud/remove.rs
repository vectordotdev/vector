use crate::kind::Collection;
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
            self.remove_inner(segments, prune);
        }
    }

    fn remove_inner(&mut self, segments: &[OwnedSegment], prune: bool) -> EmptyState {
        debug_assert!(!segments.is_empty());

        let first = segments.first().expect("must not be empty");
        let second = segments.get(1);

        if let Some(second) = second {
            unimplemented!("more segments");
        } else {
            match first {
                OwnedSegment::Field(field) => {
                    if let Some(object) = self.as_object_mut() {
                        let _result = object.known_mut().remove(&field.as_str().into());
                        object.is_empty()
                    } else {
                    }
                }
                OwnedSegment::Index(_) => unimplemented!(),
                OwnedSegment::Coalesce(_) => unimplemented!(),
                OwnedSegment::Invalid => unimplemented!(),
            }
            unimplemented!("last segment");
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
            prune: bool,
            want: Kind,
        }

        for (
            title,
            TestCase {
                kind,
                path,
                prune,
                want,
            },
        ) in [
            (
                "remove integer root",
                TestCase {
                    kind: Kind::integer(),
                    path: owned_value_path!(),
                    prune: false,
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
                    prune: false,
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
                    prune: false,
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
                    prune: false,
                    want: Kind::object(Collection::empty()),
                },
            ),
        ] {
            let mut actual = kind;
            actual.remove(&path, prune);
            if actual != want {
                panic!(
                    "Test failed: {:?}.\nExpected = {:?}\nActual = {:?}",
                    title, want, actual
                );
            }
        }
    }
}
