//! All types related to inserting one [`Kind`] into another.

use std::collections::{btree_map::Entry, BTreeMap};

use crate::kind::merge;

use super::Kind;
use lookup::{Field, Lookup, Segment};

/// The strategy to use when an inner segment in a path does not match the actual `Kind`
/// present.
///
/// For example, if a path expects an object with a given field at a certain path segment,
/// but the `Kind` defines there to be a non-object type, then one of the below actions
/// must be taken to succeed (or fail) at inserting the provided `Kind` at its final
/// destination.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InnerConflict {
    /// Keep the existing `Kind` states, but add the required state (object or array) to
    /// accomodate the final `path` structure.
    Merge(merge::Strategy),

    /// Remove the existing `Kind` states, replacing it for a singular object or array
    /// `Kind` state.
    Replace,

    /// Reject the insertion, returning an error.
    Reject,
}

/// The strategy to use when the leaf segment already has a `Kind` present.
///
/// For example, if the caller wants to insert a `Kind` at path `.foo`, but another `Kind`
/// already exists at that path, one of the below actions must be taken to resolve that
/// conflict.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LeafConflict {
    /// Keep the existing `Kind` states, but merge it with the provided `Kind`.
    Merge(merge::Strategy),

    /// Swap out the existing `Kind` for the provided one.
    Replace,

    /// Reject the insertion, returning an error.
    Reject,
}

/// The strategy to apply when inserting a new `Kind` at a given `Path`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Strategy {
    /// The strategy to apply when an inner path segment conflicts with the existing `Kind`
    /// state.
    inner_conflict: InnerConflict,

    /// The strategy to apply when the existing `Kind` state at the leaf path segment
    /// conflicts with the provided `Kind` state.
    leaf_conflict: LeafConflict,
}

/// The list of errors that can occur when `insert_at_path` fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The error variant triggered by [`LeafConflict`]'s `Reject` variant.
    LeafConflict,

    /// The error variant triggered by [`InnerConflict`]'s `Reject` variant.
    InnerConflict,

    /// The error variant triggered by a negative [`Segment::Index`] value.
    InvalidIndex,
}

impl Kind {
    /// Insert the `Kind` at the given `path` within `self`.
    ///
    /// This function behaves differently, depending on the [`InsertStrategy`] chosen.
    ///
    /// If the insertion strategy does not include a "rejection rule", then the function succeeds,
    /// unless there is a negative (unsupported) index in the path.
    ///
    /// # Errors
    ///
    /// - `LeafConflict`: inserting the `Kind` at the leaf failed due to strategy constraints.
    ///
    /// - `InnerConflict`: inserting an inner type to satisfy the provided path failed due to
    ///                    strategy constraints.
    ///
    /// - `InvalidIndex`: The provided index in the path is either negative, or out-of-bounds.
    ///
    /// # Panics
    ///
    /// Work in progress.
    #[allow(clippy::too_many_lines)]
    pub fn insert_at_path<'a>(
        &'a mut self,
        path: &'a Lookup<'a>,
        kind: Self,
        strategy: Strategy,
    ) -> Result<(), Error> {
        if path.is_root() {
            match strategy.leaf_conflict {
                LeafConflict::Merge(merge_strategy) => self.merge(kind, merge_strategy),
                LeafConflict::Replace => *self = kind,
                LeafConflict::Reject => return Err(Error::LeafConflict),
            };

            return Ok(());
        }

        let mut self_kind = self;
        let mut iter = path.iter().peekable();

        let create_inner_element = |segment: Option<&&Segment<'_>>| -> Kind {
            match segment {
                // The next segment is a field, so we'll insert an object to
                // accommodate.
                Some(segment) if segment.is_field() || segment.is_coalesce() => {
                    Kind::object(BTreeMap::default())
                }

                // The next segment is an index, so we'll insert an array.
                Some(_) => Kind::array(BTreeMap::default()),

                // There is no next segment, so we'll insert the new `Kind`.
                None => kind.clone(),
            }
        };

        let get_inner_object = |kind: &'a mut Kind, field: &'a Field<'_>| -> &'a mut Kind {
            kind.as_object_mut()
                .unwrap()
                .known_mut()
                .get_mut(&(field.into()))
                .unwrap()
        };

        let get_inner_array = |kind: &'a mut Kind, index: usize| -> &'a mut Kind {
            kind.as_array_mut()
                .unwrap()
                .known_mut()
                .get_mut(&(index.into()))
                .unwrap()
        };

        while let Some(segment) = iter.next() {
            self_kind = match segment {
                // Try finding the field in an existing object.
                Segment::Field(field) => match self_kind.object {
                    // We're already dealing with an object, so we either get, update or insert
                    // the field.
                    Some(ref mut collection) => match collection.known_mut().entry(field.into()) {
                        // The field exists in the object.
                        Entry::Occupied(entry) => match iter.peek() {
                            // There are more path segments to come, so we return the field value,
                            // and continue walking the path.
                            Some(_) => entry.into_mut(),

                            // We're done iterating the path segments, so we need to insert the
                            // provided `Kind` at this location.
                            //
                            // The next action to take depends on the configured `LeafConflict`
                            // strategy:
                            //
                            // - Merge: keep the existing `Kind`, and merge the new one into it.
                            // - Replace: swap out the existing `Kind` for the new one.
                            // - Reject: return an error.
                            None => match strategy.leaf_conflict {
                                LeafConflict::Merge(merge_strategy) => {
                                    entry.into_mut().merge(kind, merge_strategy);
                                    return Ok(());
                                }
                                LeafConflict::Replace => {
                                    *(entry.into_mut()) = kind;
                                    return Ok(());
                                }
                                LeafConflict::Reject => return Err(Error::LeafConflict),
                            },
                        },

                        // The field doesn't exist in the object, either we insert a new container
                        // if we have more segments to walk, or insert the actual `Kind`.
                        Entry::Vacant(entry) => entry.insert(create_inner_element(iter.peek())),
                    },

                    // We don't have an object, but we expect one to exist at this segment of the
                    // path. The next action to take depends on the configured `InnerConflict`
                    // strategy:
                    //
                    // - Merge: keep the existing `Kind`, and merge the new one into it.
                    // - Replace: swap out the existing `Kind` for the new one.
                    // - Reject: return an error.
                    None => match strategy.inner_conflict {
                        InnerConflict::Merge(_) => {
                            self_kind.add_object(BTreeMap::from([(
                                field.into(),
                                create_inner_element(iter.peek()),
                            )]));

                            get_inner_object(self_kind, field)
                        }

                        // We need to replace the existing value, and then, depending on the next
                        // segment in the path, move into the new value.
                        InnerConflict::Replace => {
                            *self_kind = Kind::object(BTreeMap::from([(
                                field.into(),
                                create_inner_element(iter.peek()),
                            )]));

                            get_inner_object(self_kind, field)
                        }

                        InnerConflict::Reject => return Err(Error::InnerConflict),
                    },
                },

                Segment::Coalesce(fields) => {
                    // We pick the last field in the list of coalesced fields, there is no
                    // "correct" way to handle this case, other than not supporting it.
                    let field = fields.last().expect("at least one");

                    // TODO(Jean):  This code is duplicated from the previous match arm, we'll want
                    // to DRY this up at some point.
                    match self_kind.object {
                        Some(ref mut collection) => {
                            match collection.known_mut().entry(field.into()) {
                                Entry::Occupied(entry) => match iter.peek() {
                                    Some(_) => entry.into_mut(),
                                    None => match strategy.leaf_conflict {
                                        LeafConflict::Merge(merge_strategy) => {
                                            entry.into_mut().merge(kind, merge_strategy);
                                            return Ok(());
                                        }
                                        LeafConflict::Replace => {
                                            *(entry.into_mut()) = kind;
                                            return Ok(());
                                        }
                                        LeafConflict::Reject => return Err(Error::LeafConflict),
                                    },
                                },
                                Entry::Vacant(entry) => {
                                    entry.insert(create_inner_element(iter.peek()))
                                }
                            }
                        }
                        None => match strategy.inner_conflict {
                            InnerConflict::Merge(_) => {
                                self_kind.add_object(BTreeMap::from([(
                                    field.into(),
                                    create_inner_element(iter.peek()),
                                )]));

                                get_inner_object(self_kind, field)
                            }
                            InnerConflict::Replace => {
                                *self_kind = Kind::object(BTreeMap::from([(
                                    field.into(),
                                    create_inner_element(iter.peek()),
                                )]));

                                get_inner_object(self_kind, field)
                            }
                            InnerConflict::Reject => return Err(Error::InnerConflict),
                        },
                    }
                }

                // Try finding the index in an existing array.
                Segment::Index(index) => match self_kind.array {
                    // We're already dealing with an array, so we either get, update or insert
                    // the field.
                    Some(ref mut collection) => match collection.known_mut().entry(
                        usize::try_from(*index)
                            .map_err(|_| Error::InvalidIndex)?
                            .into(),
                    ) {
                        // The field exists in the array.
                        Entry::Occupied(entry) => match iter.peek() {
                            // There are more path segments to come, so we return the field value,
                            // and continue walking the path.
                            Some(_) => entry.into_mut(),

                            // We're done iterating the path segments, so we need to insert the
                            // provided `Kind` at this location.
                            //
                            // The next action to take depends on the configured `LeafConflict`
                            // strategy:
                            //
                            // - Merge: keep the existing `Kind`, and merge the new one into it.
                            // - Replace: swap out the existing `Kind` for the new one.
                            // - Reject: return an error.
                            None => match strategy.leaf_conflict {
                                LeafConflict::Merge(merge_strategy) => {
                                    entry.into_mut().merge(kind, merge_strategy);
                                    return Ok(());
                                }
                                LeafConflict::Replace => {
                                    *(entry.into_mut()) = kind;
                                    return Ok(());
                                }
                                LeafConflict::Reject => return Err(Error::LeafConflict),
                            },
                        },

                        // The field doesn't exist in the array, either we insert a new container
                        // if we have more segments to walk, or insert the actual `Kind`.
                        Entry::Vacant(entry) => entry.insert(create_inner_element(iter.peek())),
                    },

                    // We don't have an array, but we expect one to exist at this segment of the
                    // path. The next action to take depends on the configured `InnerConflict`
                    // strategy:
                    //
                    // - Merge: keep the existing `Kind`, and merge the new one into it.
                    // - Replace: swap out the existing `Kind` for the new one.
                    // - Reject: return an error.
                    None => match strategy.inner_conflict {
                        InnerConflict::Merge(_) => {
                            let index = usize::try_from(*index).map_err(|_| Error::InvalidIndex)?;

                            self_kind.add_array(BTreeMap::from([(
                                index.into(),
                                create_inner_element(iter.peek()),
                            )]));

                            get_inner_array(self_kind, index)
                        }
                        InnerConflict::Replace => {
                            let index = usize::try_from(*index).map_err(|_| Error::InvalidIndex)?;

                            *self_kind = Self::array(BTreeMap::from([(
                                index.into(),
                                create_inner_element(iter.peek()),
                            )]));

                            *self_kind = Self::array(BTreeMap::default());
                            get_inner_array(self_kind, index)
                        }
                        InnerConflict::Reject => return Err(Error::InnerConflict),
                    },
                },
            };
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use lookup::LookupBuf;

    use crate::kind::Collection;

    use super::*;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_update_at_path() {
        struct TestCase {
            this: Kind,
            path: LookupBuf,
            kind: Kind,
            updated: bool,
            mutated: Kind,
        }

        for (
            title,
            TestCase {
                mut this,
                path,
                kind,
                updated,
                mutated,
            },
        ) in HashMap::from([
            (
                "root path",
                TestCase {
                    this: Kind::bytes(),
                    path: LookupBuf::root(),
                    kind: Kind::integer(),
                    updated: true,
                    mutated: Kind::integer(),
                },
            ),
            (
                "object w/o known",
                TestCase {
                    this: Kind::object(Collection::json()),
                    path: LookupBuf::root(),
                    kind: Kind::object(Collection::any()),
                    updated: true,
                    mutated: Kind::object(Collection::any()),
                },
            ),
            (
                "negative indexing",
                TestCase {
                    this: Kind::array(BTreeMap::from([(1.into(), Kind::timestamp())])),
                    path: LookupBuf::from_str("[-1]").unwrap(),
                    kind: Kind::object(Collection::any()),
                    updated: false,
                    mutated: Kind::array(BTreeMap::from([(1.into(), Kind::timestamp())])),
                },
            ),
            (
                "object w/ matching path",
                TestCase {
                    this: Kind::object(BTreeMap::from([("foo".into(), Kind::integer())])),
                    path: "foo".into(),
                    kind: Kind::object(Collection::any()),
                    updated: true,
                    mutated: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(Collection::any()),
                    )])),
                },
            ),
            (
                "complex pathing",
                TestCase {
                    this: Kind::object(BTreeMap::from([(
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
                    kind: Kind::object(BTreeMap::from([("baz".into(), Kind::bytes().or_regex())])),
                    updated: true,
                    mutated: Kind::object(BTreeMap::from([(
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
                                            Kind::bytes().or_regex(),
                                        )])),
                                    ),
                                    ("qux".into(), Kind::boolean()),
                                ])),
                            ),
                        ])),
                    )])),
                },
            ),
        ]) {
            let strategy = Strategy {
                inner_conflict: InnerConflict::Reject,
                leaf_conflict: LeafConflict::Replace,
            };

            let got = this.insert_at_path(&path.to_lookup(), kind, strategy);

            assert_eq!(got.is_ok(), updated, "updated: {}", title);
            assert_eq!(this, mutated, "mutated: {}", title);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_insert_at_path() {
        struct TestCase {
            this: Kind,
            path: LookupBuf,
            kind: Kind,
            strategy: Strategy,
            mutated: Kind,
            result: Result<(), Error>,
        }

        for (
            title,
            TestCase {
                mut this,
                path,
                kind,
                strategy,
                mutated,
                result,
            },
        ) in HashMap::from([
            (
                "root path /w inner reject/leaf reject",
                TestCase {
                    this: Kind::bytes(),
                    path: LookupBuf::root(),
                    kind: Kind::integer(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Reject,
                        leaf_conflict: LeafConflict::Reject,
                    },
                    mutated: Kind::bytes(),
                    result: Err(Error::LeafConflict),
                },
            ),
            (
                "root path /w inner reject/leaf replace",
                TestCase {
                    this: Kind::bytes(),
                    path: LookupBuf::root(),
                    kind: Kind::integer(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Reject,
                        leaf_conflict: LeafConflict::Replace,
                    },
                    mutated: Kind::integer(),
                    result: Ok(()),
                },
            ),
            (
                "root path /w inner reject/leaf merge",
                TestCase {
                    this: Kind::bytes(),
                    path: LookupBuf::root(),
                    kind: Kind::integer(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Reject,
                        leaf_conflict: LeafConflict::Merge(merge::Strategy {
                            depth: merge::Depth::Shallow,
                            indices: merge::Indices::Keep,
                        }),
                    },
                    mutated: Kind::integer().or_bytes(),
                    result: Ok(()),
                },
            ),
            (
                "nested path",
                TestCase {
                    this: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([(
                            1.into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::boolean())])),
                        )])),
                    )])),
                    path: LookupBuf::from_str(".foo[1].bar").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Reject,
                        leaf_conflict: LeafConflict::Replace,
                    },
                    mutated: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([(
                            1.into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::timestamp())])),
                        )])),
                    )])),
                    result: Ok(()),
                },
            ),
            (
                "nested path /w inner reject/leaf replace",
                TestCase {
                    this: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([(
                            1.into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::boolean())])),
                        )])),
                    )])),
                    path: LookupBuf::from_str(".foo.baz.bar").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Reject,
                        leaf_conflict: LeafConflict::Replace,
                    },
                    mutated: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([(
                            1.into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::boolean())])),
                        )])),
                    )])),
                    result: Err(Error::InnerConflict),
                },
            ),
            (
                "nested path /w inner replace/leaf reject",
                TestCase {
                    this: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([(
                            1.into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::boolean())])),
                        )])),
                    )])),
                    path: LookupBuf::from_str(".foo.baz.bar").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Replace,
                        leaf_conflict: LeafConflict::Reject,
                    },
                    mutated: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([(
                            "baz".into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::timestamp())])),
                        )])),
                    )])),
                    result: Ok(()),
                },
            ),
            (
                "coalesced path /w inner replace/leaf reject",
                TestCase {
                    this: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([(
                            1.into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::boolean())])),
                        )])),
                    )])),
                    path: LookupBuf::from_str(".(fitz | foo).baz.bar").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Replace,
                        leaf_conflict: LeafConflict::Reject,
                    },
                    mutated: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::object(BTreeMap::from([(
                            "baz".into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::timestamp())])),
                        )])),
                    )])),
                    result: Ok(()),
                },
            ),
        ]) {
            let got = this.insert_at_path(&path.to_lookup(), kind, strategy);

            assert_eq!(got, result, "{}", title);
            assert_eq!(this, mutated, "{}", title);
        }
    }
}
