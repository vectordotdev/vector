use std::collections::BTreeMap;

use super::{Collection, Kind};
use lookup::{Lookup, Segment};

impl Kind {
    /// Find the [`Kind`] at the given path.
    ///
    /// If the path points to root, then `self` is returned, otherwise `None` is returned if `Kind`
    /// isn't an object or array, or if the path points to a non-existing field/index in the
    /// object/array.
    ///
    /// Negative indexing always returns `None`, as the type system doesn't know the exact size of
    /// an array, and thus cannot count backward.
    #[must_use]
    pub fn find_at_path(&self, path: &Lookup<'_>) -> Option<&Kind> {
        if path.is_root() {
            return Some(self);
        }

        let mut kind = self;
        for segment in path.iter() {
            kind = match segment {
                // Try finding the field in the existing object.
                Segment::Field(field) => kind
                    .object
                    .as_ref()
                    .and_then(|collection| collection.known().get(&(field.into())))?,

                // Try finding one of the fields in the existing object.
                Segment::Coalesce(fields) => kind.object.as_ref().and_then(|collection| {
                    let field = fields
                        .iter()
                        .find(|field| collection.known().contains_key(&((*field).into())))?;

                    collection.known().get(&(field.into()))
                })?,

                // Try finding the index in the existing array.
                Segment::Index(index) => usize::try_from(*index).ok().and_then(|index| {
                    kind.array
                        .as_ref()
                        .and_then(|collection| collection.known().get(&(index.into())))
                })?,
            };
        }

        Some(kind)
    }

    /// Nest the given [`Kind`] into a provided path.
    ///
    /// For example, given an `integer` kind and a path `.foo`, a new `Kind` is returned that is
    /// known to be an object, of which the `foo` field is known to be an `integer`.
    #[must_use]
    pub fn nest_at_path(mut self, path: &Lookup<'_>) -> Self {
        fn object_from_field(field: &lookup::Field<'_>, kind: Kind) -> Kind {
            let map = BTreeMap::from([(field.into(), kind)]);
            Kind::object(map)
        }

        for segment in path.iter().rev() {
            match segment {
                Segment::Field(field) => {
                    self = object_from_field(field, self);
                }
                Segment::Coalesce(fields) => {
                    // We pick the last field in the list of coalesced fields, there is no
                    // "correct" way to handle this case, other than not supporting it, or making
                    // this method call fallible.
                    let field = fields.last().expect("at least one");
                    self = object_from_field(field, self);
                }
                Segment::Index(index) => {
                    // Try to get a valid `usize`-index from the `isize` index. For invalid ones
                    // (e.g. negative indices, or when the value is out of range), mark the entire
                    // array contents as unknown, since there's no way to determine which index has
                    // the given type.
                    let collection = if let Ok(index) = usize::try_from(*index) {
                        let map = BTreeMap::from([(index.into(), self)]);
                        Collection::from(map)
                    } else {
                        Collection::any()
                    };

                    self = Self::array(collection);
                }
            }
        }

        self
    }

    /// Remove, and return the `Kind` at the given `path`.
    ///
    /// For arrays, indices are shifted back if any element before the last is removed.
    ///
    /// If the `kind` is a non-collection type, or the path points to a non-existing location in
    /// a collection, this method returns `None`.
    ///
    /// Negative indexing always returns `None`, as the type system doesn't know the exact size of
    /// an array, and thus cannot count backward.
    ///
    /// # Panics
    ///
    /// `path` must not point to root. This is because it's ambiguous whether to return the
    /// root-level `object` or `array`, if `Kind` has both defined.
    ///
    /// Use `into_object` or `into_array` if you need the root-level object or array.
    pub fn remove_at_path(&mut self, path: &Lookup<'_>) -> Option<Self> {
        // Cannot remove using root-path.
        if path.is_root() {
            panic!("cannot remove root path");
        }

        let mut kind = self;
        let mut iter = path.iter().peekable();

        while let Some(segment) = iter.next() {
            let last = iter.peek().is_none();

            kind = match segment {
                // Remove and return the final field.
                Segment::Field(field) if last => {
                    return kind
                        .object
                        .as_mut()
                        .and_then(|collection| collection.known_mut().remove(&(field.into())))
                }

                // Try finding the field in the existing object.
                Segment::Field(field) => kind
                    .object
                    .as_mut()
                    .and_then(|collection| collection.known_mut().get_mut(&(field.into())))?,

                // Remove and return the final matching field.
                Segment::Coalesce(fields) if last => {
                    return kind.object.as_mut().and_then(|collection| {
                        fields
                            .iter()
                            .find_map(|field| collection.known_mut().remove(&(field.into())))
                    })
                }

                // Try finding one of the fields in the existing object.
                Segment::Coalesce(fields) => kind.object.as_mut().and_then(|collection| {
                    let field = fields
                        .iter()
                        .find(|field| collection.known().contains_key(&((*field).into())))?;

                    collection.known_mut().get_mut(&(field.into()))
                })?,

                // Remove and return the final matching index. Also down-shift any indices
                // following the removed index.
                Segment::Index(index) if last => {
                    let index = usize::try_from(*index).ok()?;

                    let kind = kind.array.as_mut().and_then(|collection| {
                        let kind = collection.known_mut().remove(&(index.into()))?;

                        // Get all indices that we need to down-shift after removing an element.
                        let indices = collection
                            .known()
                            .iter()
                            .filter_map(|(idx, _)| (usize::from(*idx) > index).then(|| idx))
                            .copied()
                            .collect::<Vec<_>>();

                        // Remove all elements for which we need to down-shift the indices.
                        let mut entries = vec![];
                        for index in indices {
                            let kind = collection.known_mut().remove(&index).expect("exists");
                            entries.push((usize::from(index), kind));
                        }

                        // Re-insert all elements with the correct index applied.
                        for (i, kind) in entries {
                            collection.known_mut().insert((i - 1).into(), kind);
                        }

                        Some(kind)
                    })?;

                    return Some(kind);
                }

                // Try finding the index in the existing array.
                Segment::Index(index) => usize::try_from(*index).ok().and_then(|index| {
                    kind.array
                        .as_mut()
                        .and_then(|collection| collection.known_mut().get_mut(&(index.into())))
                })?,
            };
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use lookup::LookupBuf;

    use super::*;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_find_at_path() {
        struct TestCase {
            kind: Kind,
            path: LookupBuf,
            want: Option<Kind>,
        }

        for (title, TestCase { kind, path, want }) in HashMap::from([
            (
                "primitive",
                TestCase {
                    kind: Kind::bytes(),
                    path: "foo".into(),
                    want: None,
                },
            ),
            (
                "multiple primitives",
                TestCase {
                    kind: Kind::integer().or_regex(),
                    path: "foo".into(),
                    want: None,
                },
            ),
            (
                "object w/ matching path",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                    path: "foo".into(),
                    want: Some(Kind::integer()),
                },
            ),
            (
                "object w/o matching path",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                    path: "bar".into(),
                    want: None,
                },
            ),
            (
                "array w/ matching path",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[1]").unwrap(),
                    want: Some(Kind::integer()),
                },
            ),
            (
                "array w/o matching path",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    want: None,
                },
            ),
            (
                "array w/ matching path, shifting indices",
                TestCase {
                    kind: Kind::array(BTreeMap::from([
                        (1.into(), Kind::integer()),
                        (2.into(), Kind::bytes()),
                        (3.into(), Kind::boolean()),
                        (4.into(), Kind::regex()),
                    ])),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    want: Some(Kind::bytes()),
                },
            ),
            (
                "array w/ negative indexing",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[-1]").unwrap(),
                    want: None,
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
                    want: Some(Kind::object(BTreeMap::from([(
                        "baz".into(),
                        Kind::integer().or_regex(),
                    )]))),
                },
            ),
        ]) {
            assert_eq!(
                kind.find_at_path(&path.to_lookup()),
                want.as_ref(),
                "returned: {}",
                title
            );
        }
    }

    #[test]
    fn test_nest_at_path() {
        struct TestCase {
            kind: Kind,
            path: LookupBuf,
            want: Kind,
        }

        for (title, TestCase { kind, path, want }) in HashMap::from([
            (
                "single-level object",
                TestCase {
                    kind: Kind::bytes(),
                    path: "foo".into(),
                    want: Kind::empty().or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                },
            ),
            (
                "multi-level object",
                TestCase {
                    kind: Kind::boolean(),
                    path: LookupBuf::from_str("foo.bar").unwrap(),
                    want: Kind::empty().or_object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([("bar".into(), Kind::boolean())])),
                    )])),
                },
            ),
            (
                "array positive index",
                TestCase {
                    kind: Kind::integer(),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    want: Kind::empty().or_array(BTreeMap::from([(2.into(), Kind::integer())])),
                },
            ),
            (
                "array negative index",
                TestCase {
                    kind: Kind::integer(),
                    path: LookupBuf::from_str("[-2]").unwrap(),
                    want: Kind::empty().or_array(BTreeMap::default()),
                },
            ),
            (
                "mixed path",
                TestCase {
                    kind: Kind::integer().or_bytes(),
                    path: LookupBuf::from_str(".foo.bar[1].baz").unwrap(),
                    want: Kind::empty().or_object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([(
                            "bar".into(),
                            Kind::array(BTreeMap::from([(
                                1.into(),
                                Kind::object(BTreeMap::from([(
                                    "baz".into(),
                                    Kind::integer().or_bytes(),
                                )])),
                            )])),
                        )])),
                    )])),
                },
            ),
        ]) {
            assert_eq!(kind.nest_at_path(&path.to_lookup()), want, "{}", title);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_remove_at_path() {
        struct TestCase {
            kind: Kind,
            path: LookupBuf,
            returned: Option<Kind>,
            mutated: Kind,
        }

        for (
            title,
            TestCase {
                mut kind,
                path,
                returned,
                mutated,
            },
        ) in HashMap::from([
            (
                "primitive",
                TestCase {
                    kind: Kind::bytes(),
                    path: "foo".into(),
                    returned: None,
                    mutated: Kind::bytes(),
                },
            ),
            (
                "multiple primitives",
                TestCase {
                    kind: Kind::integer().or_regex(),
                    path: "foo".into(),
                    returned: None,
                    mutated: Kind::integer().or_regex(),
                },
            ),
            (
                "object w/ matching path",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                    path: "foo".into(),
                    returned: Some(Kind::integer()),
                    mutated: Kind::object(BTreeMap::default()),
                },
            ),
            (
                "object w/o matching path",
                TestCase {
                    kind: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                    path: "bar".into(),
                    returned: None,
                    mutated: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                },
            ),
            (
                "array w/ matching path",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[1]").unwrap(),
                    returned: Some(Kind::integer()),
                    mutated: Kind::array(BTreeMap::default()),
                },
            ),
            (
                "array w/o matching path",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    returned: None,
                    mutated: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                },
            ),
            (
                "array w/ negative indexing",
                TestCase {
                    kind: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                    path: LookupBuf::from_str("[-1]").unwrap(),
                    returned: None,
                    mutated: Kind::array(BTreeMap::from([(1.into(), Kind::integer())])),
                },
            ),
            (
                "array w/ matching path, shifting indices",
                TestCase {
                    kind: Kind::array(BTreeMap::from([
                        (1.into(), Kind::integer()),
                        (2.into(), Kind::bytes()),
                        (3.into(), Kind::boolean()),
                        (4.into(), Kind::regex()),
                    ])),
                    path: LookupBuf::from_str("[2]").unwrap(),
                    returned: Some(Kind::bytes()),
                    mutated: Kind::array(BTreeMap::from([
                        (1.into(), Kind::integer()),
                        (2.into(), Kind::boolean()),
                        (3.into(), Kind::regex()),
                    ])),
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
                    returned: Some(Kind::object(BTreeMap::from([(
                        "baz".into(),
                        Kind::integer().or_regex(),
                    )]))),
                    mutated: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([
                            (1.into(), Kind::integer()),
                            (
                                2.into(),
                                Kind::object(BTreeMap::from([("qux".into(), Kind::boolean())])),
                            ),
                        ])),
                    )])),
                },
            ),
        ]) {
            let got = kind.remove_at_path(&path.to_lookup());

            assert_eq!(got, returned, "returned: {}", title);
            assert_eq!(kind, mutated, " mutated: {}", title);
        }
    }
}
