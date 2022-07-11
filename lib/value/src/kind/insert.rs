//! All types related to inserting one [`Kind`] into another.

use std::collections::{btree_map::Entry, BTreeMap, VecDeque};

use lookup::{Field, Lookup, Segment};

use super::Kind;
use crate::kind::merge;

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

impl InnerConflict {
    /// Check if the active strategy is "merge".
    #[must_use]
    pub const fn is_merge(&self) -> bool {
        matches!(self, Self::Merge(_))
    }

    /// Check if the active strategy is "replace".
    #[must_use]
    pub const fn is_replace(&self) -> bool {
        matches!(self, Self::Replace)
    }

    /// Check if the active strategy is "reject".
    #[must_use]
    pub const fn is_reject(&self) -> bool {
        matches!(self, Self::Reject)
    }
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

impl LeafConflict {
    /// Check if the active strategy is "merge".
    #[must_use]
    pub const fn is_merge(&self) -> bool {
        matches!(self, Self::Merge(_))
    }

    /// Check if the active strategy is "replace".
    #[must_use]
    pub const fn is_replace(&self) -> bool {
        matches!(self, Self::Replace)
    }

    /// Check if the active strategy is "reject".
    #[must_use]
    pub const fn is_reject(&self) -> bool {
        matches!(self, Self::Reject)
    }
}

/// The strategy to use when a given path contains a coalesced segment.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CoalescedPath {
    /// Insert the required `Kind` into all *valid* coalesced fields.
    ///
    /// This insertion strategy takes into account what the runtime behavior will be of querying
    /// the given path.
    ///
    /// That is, for path `.(foo | bar)`, with a boolean `Kind`, after insertion, a query for
    /// `.foo` will *always* match (and return a boolean), and thus the second `bar` field will
    /// never trigger, and thus no type information is inserted.
    InsertValid,

    /// Insert the required `Kind` into *all* coalesced fields.
    ///
    /// Meaning, for path `.foo.(bar | baz).qux and a boolean `Kind`, the result will be:
    ///
    ///   .foo         = object
    ///   .foo.bar     = object
    ///   .foo.baz     = object
    ///   .foo.bar.qux = boolean
    ///   .foo.baz.qux = boolean
    InsertAll,

    /// Reject coalesced path segments during insertion, returning an error.
    Reject,
}

impl CoalescedPath {
    /// Check if the active strategy is "insert valid".
    #[must_use]
    pub const fn is_insert_valid(&self) -> bool {
        matches!(self, Self::InsertValid)
    }

    /// Check if the active strategy is "insert all".
    #[must_use]
    pub const fn is_insert_all(&self) -> bool {
        matches!(self, Self::InsertAll)
    }

    /// Check if the active strategy is "reject".
    #[must_use]
    pub const fn is_reject(&self) -> bool {
        matches!(self, Self::Reject)
    }
}

/// The strategy to apply when inserting a new `Kind` at a given `Path`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Strategy {
    /// The strategy to apply when an inner path segment conflicts with the existing `Kind`
    /// state.
    pub inner_conflict: InnerConflict,

    /// The strategy to apply when the existing `Kind` state at the leaf path segment
    /// conflicts with the provided `Kind` state.
    pub leaf_conflict: LeafConflict,

    /// The strategy to apply when the given `Path` contains a "coalesced" segment.
    pub coalesced_path: CoalescedPath,
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

    /// The error variant triggered by [`CoalescedPath`]'s `Reject` variant.
    CoalescedPathSegment,
}

impl Kind {
    /// Insert the `Kind` at the given `path` within `self`.
    ///
    /// This function behaves differently, depending on the [`Strategy`] chosen.
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
    /// - `CoalescedPathSegment`: The provided `path` contains a coalesced segment, but the
    ///                           configured strategy prohibits this segment type.
    #[allow(clippy::too_many_lines, clippy::missing_panics_doc)]
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

        let create_inner_element = |segment: Option<&&Segment<'_>>| -> Self {
            match segment {
                // The next segment is a field, so we'll insert an object to
                // accommodate.
                Some(segment) if segment.is_field() || segment.is_coalesce() => {
                    Self::object(BTreeMap::default())
                }

                // The next segment is an index, so we'll insert an array.
                Some(_) => Self::array(BTreeMap::default()),

                // There is no next segment, so we'll insert the new `Kind`.
                None => kind.clone(),
            }
        };

        let get_inner_object = |kind: &'a mut Self, field: &'a Field<'_>| -> &'a mut Self {
            kind.as_object_mut()
                .unwrap()
                .known_mut()
                .get_mut(&(field.into()))
                .unwrap()
        };

        let get_inner_array = |kind: &'a mut Self, index: usize| -> &'a mut Self {
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
                    // path.
                    //
                    // The next action depends on if there are more path segments to iterate.
                    None => {
                        // We'll add the object state to the existing `Kind` states.
                        let mut merge = |segment: Option<&Segment<'_>>| {
                            self_kind.add_object(BTreeMap::from([(
                                field.into(),
                                create_inner_element(segment.as_ref()),
                            )]));

                            Ok(())
                        };

                        // We'll replace the existing `Kind` state with an object.
                        let replace = |self_kind: &mut Self, segment: Option<&Segment<'_>>| {
                            *self_kind = Self::object(BTreeMap::from([(
                                field.into(),
                                create_inner_element(segment.as_ref()),
                            )]));

                            Ok(())
                        };

                        match iter.peek() {
                            // There are more path segments to follow, use the inner strategy.
                            Some(segment) => {
                                let _ = match strategy.inner_conflict {
                                    InnerConflict::Merge(_) => merge(Some(segment)),
                                    InnerConflict::Replace => replace(self_kind, Some(segment)),
                                    InnerConflict::Reject => return Err(Error::InnerConflict),
                                };

                                get_inner_object(self_kind, field)
                            }

                            // There are no more path segments to follow, so we insert and return.
                            None => match strategy.leaf_conflict {
                                LeafConflict::Merge(_) => return merge(None),
                                LeafConflict::Replace => return replace(self_kind, None),
                                LeafConflict::Reject => return Err(Error::LeafConflict),
                            },
                        }
                    }
                },

                // Coalesced path segments are rather complex to grok, so what follows is
                // a detailed description of what happens in different situations.
                //
                // 1. The most straight-forward resolution is when a coalesced path segment exists,
                //    but the `CoalescedPath` strategy is set to `Reject`. In that case, the method
                //    returns the `CoalescedPathSegment` error.
                //
                // 2. Next, if the first field in a coalesced segment points to an existing field
                //    in the `Kind`, _and_ that field can never be `null`, then we know the
                //    coalescing is similar to a regular field segment.
                //
                //    Take this example, given the path `.(foo | bar)` and `Kind`:
                //
                //    ```
                //    { object => { "foo": bytes, "bar": integer } }
                //    ```
                //
                //    In this case, because the top-level kind is an object, and it has a field
                //    `foo` which cannot be `null`, we know that `.(foo | bar)` will always trigger
                //    the `foo` field, and never trigger `bar`, because the first cannot return
                //    `null`.
                //
                // 3. What is in the above example, the top-level kind could be something other
                //    than an object?
                //
                //    ```
                //    { bytes | object => { "foo": bytes, "bar": integer } }
                //    ```
                //
                //    In this case, the call to `.(foo | bar)` can return `null`, because the
                //    top-level kind might not be an object, and thus the field query wouldn't
                //    return any data.
                //
                // 4. What if instead `foo` could be null?
                //
                //    ```
                //    { object => { "foo": bytes | null, "bar": integer } }
                //    ```
                //
                //    In this case, we would have to return a kind that can either be `bytes` or
                //    `integer`, because querying `foo` might return `null`.
                //
                // 5. What if we kept the previous kind, but changed the path to `.(foo | baz)`?
                //
                //    In this case, `foo` can be null, but `baz` does not exist, so will never
                //    return a kind. In this situation, we have to look at the "unknown" field kind
                //    configured within the object. That is, if we don't know a `baz` field, but we
                //    do know that "any unknown field can be a float", then the new kind would
                //    either be `bytes` or `float`.
                //
                //    If no unknown kind is defined (i.e. it is `None`), then it means we are sure
                //    that there are no fields other than the "known" ones configured, and so the
                //    resolution would be either `bytes` or `null`.
                Segment::Coalesce(_) if strategy.coalesced_path.is_reject() => {
                    return Err(Error::CoalescedPathSegment)
                }

                // We're dealing with multiple fields in this segment. This requires us to
                // recursively call this `insert_at_path` function for each field.
                Segment::Coalesce(fields) => {
                    for field in fields {
                        let mut segments = iter.clone().cloned().collect::<VecDeque<_>>();
                        segments.push_front(Segment::Field(field.clone()));
                        let path = Lookup::from(segments);

                        self_kind.insert_at_path(&path, kind.clone(), strategy)?;

                        let is_nullable = self_kind
                            .as_object()
                            .unwrap()
                            .known()
                            .get(&(field.into()))
                            .unwrap()
                            .contains_null();

                        // The above-inserted `kind` cannot be `null` at runtime, so we are
                        // guaranteed that this field will always match for the coalesced segment,
                        // there's no need to iterate the subsequent fields in the segment.
                        //
                        // This only applies for the "insert valid" strategy, the "insert all"
                        // strategy will continue to insert *all* fields in the coalesced segment.
                        if strategy.coalesced_path.is_insert_valid() && !is_nullable {
                            break;
                        }
                    }

                    return Ok(());
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

    use super::*;
    use crate::kind::Collection;

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
                coalesced_path: CoalescedPath::Reject,
            };

            let got = this.insert_at_path(&path.to_lookup(), kind, strategy);

            assert_eq!(got.is_ok(), updated, "updated: {}", title);
            assert_eq!(this, mutated, "mutated: {}", title);
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_coalesced_path() {
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
                "reject strategy",
                TestCase {
                    this: Kind::boolean(),
                    path: LookupBuf::from_str(".(fitz | foo)").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Replace,
                        leaf_conflict: LeafConflict::Replace,
                        coalesced_path: CoalescedPath::Reject,
                    },
                    mutated: Kind::boolean(),
                    result: Err(Error::CoalescedPathSegment),
                },
            ),
            (
                "single coalesced path / two variants / insert_valid",
                TestCase {
                    this: Kind::boolean(),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Merge(merge::Strategy {
                            collisions: merge::CollisionStrategy::Overwrite,
                            indices: merge::Indices::Keep,
                        }),
                        leaf_conflict: LeafConflict::Merge(merge::Strategy {
                            collisions: merge::CollisionStrategy::Overwrite,
                            indices: merge::Indices::Keep,
                        }),
                        coalesced_path: CoalescedPath::InsertValid,
                    },
                    mutated: Kind::object(BTreeMap::from([("foo".into(), Kind::timestamp())]))
                        .or_boolean(),
                    result: Ok(()),
                },
            ),
            (
                "coalesced path, first field always matches",
                TestCase {
                    this: Kind::object(BTreeMap::from([("foo".into(), Kind::regex())])),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Merge(merge::Strategy {
                            collisions: merge::CollisionStrategy::Overwrite,
                            indices: merge::Indices::Keep,
                        }),
                        leaf_conflict: LeafConflict::Merge(merge::Strategy {
                            collisions: merge::CollisionStrategy::Overwrite,
                            indices: merge::Indices::Keep,
                        }),
                        coalesced_path: CoalescedPath::InsertValid,
                    },
                    mutated: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::regex().or_timestamp(),
                    )])),
                    result: Ok(()),
                },
            ),
            (
                "coalesced path, first field can be null",
                TestCase {
                    this: Kind::object(BTreeMap::from([("foo".into(), Kind::null())])),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Merge(merge::Strategy {
                            collisions: merge::CollisionStrategy::Overwrite,
                            indices: merge::Indices::Keep,
                        }),
                        leaf_conflict: LeafConflict::Merge(merge::Strategy {
                            collisions: merge::CollisionStrategy::Overwrite,
                            indices: merge::Indices::Keep,
                        }),
                        coalesced_path: CoalescedPath::InsertValid,
                    },
                    mutated: Kind::object(BTreeMap::from([
                        ("foo".into(), Kind::null().or_timestamp()),
                        ("bar".into(), Kind::timestamp()),
                    ])),
                    result: Ok(()),
                },
            ),
        ]) {
            let got = this.insert_at_path(&path.to_lookup(), kind, strategy);

            assert_eq!(got, result, "{}", title);
            assert_eq!(this, mutated, "{}", title);
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
                        coalesced_path: CoalescedPath::Reject,
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
                        coalesced_path: CoalescedPath::Reject,
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
                            collisions: merge::CollisionStrategy::Overwrite,
                            indices: merge::Indices::Keep,
                        }),
                        coalesced_path: CoalescedPath::Reject,
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
                        coalesced_path: CoalescedPath::Reject,
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
                        coalesced_path: CoalescedPath::Reject,
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
                        coalesced_path: CoalescedPath::Reject,
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
                "coalesced path reject",
                TestCase {
                    this: Kind::bytes(),
                    path: LookupBuf::from_str(".(foo | bar)").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Replace,
                        leaf_conflict: LeafConflict::Replace,
                        coalesced_path: CoalescedPath::Reject,
                    },
                    mutated: Kind::bytes(),
                    result: Err(Error::CoalescedPathSegment),
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
                        coalesced_path: CoalescedPath::InsertAll,
                    },
                    mutated: Kind::object(BTreeMap::from([
                        (
                            "fitz".into(),
                            Kind::object(BTreeMap::from([(
                                "baz".into(),
                                Kind::object(BTreeMap::from([("bar".into(), Kind::timestamp())])),
                            )])),
                        ),
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([(
                                "baz".into(),
                                Kind::object(BTreeMap::from([("bar".into(), Kind::timestamp())])),
                            )])),
                        ),
                    ])),
                    result: Ok(()),
                },
            ),
            (
                "coalesced path w/o object",
                TestCase {
                    this: Kind::bytes(),
                    path: LookupBuf::from_str(".(fitz | foo).bar.baz").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Replace,
                        leaf_conflict: LeafConflict::Replace,
                        coalesced_path: CoalescedPath::InsertAll,
                    },
                    mutated: Kind::object(BTreeMap::from([
                        (
                            "fitz".into(),
                            Kind::object(BTreeMap::from([(
                                "bar".into(),
                                Kind::object(BTreeMap::from([("baz".into(), Kind::timestamp())])),
                            )])),
                        ),
                        (
                            "foo".into(),
                            Kind::object(BTreeMap::from([(
                                "bar".into(),
                                Kind::object(BTreeMap::from([("baz".into(), Kind::timestamp())])),
                            )])),
                        ),
                    ])),
                    result: Ok(()),
                },
            ),
            (
                "coalesced path at leaf /w object",
                TestCase {
                    this: Kind::object(BTreeMap::from([(
                        "foo".into(),
                        Kind::array(BTreeMap::from([(
                            1.into(),
                            Kind::object(BTreeMap::from([("bar".into(), Kind::boolean())])),
                        )])),
                    )])),
                    path: LookupBuf::from_str(".(fitz | foo)").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Merge(merge::Strategy {
                            collisions: merge::CollisionStrategy::Union,
                            indices: merge::Indices::Append,
                        }),
                        leaf_conflict: LeafConflict::Merge(merge::Strategy {
                            collisions: merge::CollisionStrategy::Union,
                            indices: merge::Indices::Append,
                        }),
                        coalesced_path: CoalescedPath::InsertAll,
                    },
                    mutated: Kind::object(BTreeMap::from([
                        ("fitz".into(), Kind::timestamp()),
                        (
                            "foo".into(),
                            Kind::timestamp().or_array(BTreeMap::from([(
                                1.into(),
                                Kind::object(BTreeMap::from([("bar".into(), Kind::boolean())])),
                            )])),
                        ),
                    ])),
                    result: Ok(()),
                },
            ),
            (
                "coalesced path at leaf w/o object",
                TestCase {
                    this: Kind::boolean(),
                    path: LookupBuf::from_str(".(fitz | foo)").unwrap(),
                    kind: Kind::timestamp(),
                    strategy: Strategy {
                        inner_conflict: InnerConflict::Replace,
                        leaf_conflict: LeafConflict::Merge(merge::Strategy {
                            collisions: merge::CollisionStrategy::Overwrite,
                            indices: merge::Indices::Keep,
                        }),
                        coalesced_path: CoalescedPath::InsertAll,
                    },
                    mutated: Kind::object(BTreeMap::from([
                        ("fitz".into(), Kind::timestamp()),
                        ("foo".into(), Kind::timestamp()),
                    ]))
                    .or_boolean(),
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
