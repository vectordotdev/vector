use crate::kind::Collection;
use crate::Kind;
use lookup::lookup_v2::OwnedSegment;
use lookup::OwnedValuePath;

impl Kind {
    /// Removes the `Kind` at the given `path` within `self`.
    /// This has the same behavior as `Value::remove`.
    #[allow(clippy::needless_pass_by_value)] // only reference types implement Path
    pub fn remove(&mut self, path: &OwnedValuePath, prune: bool) {
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
        // let mut this = self;

        // for segment in segments {
        //     match segment {
        //         OwnedSegment::Field(field) => unimplemented!(),
        //         OwnedSegment::Index(index) => unimplemented!(),
        //         OwnedSegment::Coalesce(fields) => unimplemented!(),
        //         OwnedSegment::Invalid => {
        //             // Same behavior as `Value::remove`, do nothing. Eventually
        //             // this variant should be removed from `OwnedValuePath`.
        //             return;
        //         }
        //     }
        // }
    }

    fn remove_inner(&mut self, segments: &[OwnedSegment], prune: bool) -> RemoveResult {
        debug_assert!(!segments.is_empty());

        unimplemented!()
    }
}

struct RemoveResult {
    is_empty: bool,
}

#[cfg(test)]
mod test {
    use super::*;
    use lookup::owned_value_path;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_at_path() {
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
        ) in [(
            "remove integer root",
            TestCase {
                kind: Kind::integer(),
                path: owned_value_path!(),
                prune: false,
                want: Kind::null(),
            },
        )] {
            assert_eq!(kind.at_path(&path), want, "test: {}", title);
        }
    }
}
