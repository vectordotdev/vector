//! All types related to inserting one [`Kind`] into another.

use lookup::lookup_v2::{BorrowedSegment, Path};
use lookup::{Field, Lookup, Segment};
use std::collections::{btree_map::Entry, BTreeMap, VecDeque};

use super::Kind;
use crate::kind::{merge, Collection};
use lookup::path;

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

    /// Insert the `Kind` at the given `path` within `self`.
    /// This has the same behavior as setting a value at a given path at runtime.
    pub fn insert<'a>(&'a mut self, path: impl Path<'a>, kind: Self) {
        // need to re-bind self to make a mutable reference
        let mut self_kind = self;

        let mut iter = path.segment_iter().peekable();

        while let Some(segment) = iter.next() {
            self_kind = match segment {
                BorrowedSegment::Field(field) => {
                    // field insertion converts the value to an object, so remove all other types
                    *self_kind = Kind::object(
                        self_kind
                            .object
                            .clone()
                            .unwrap_or_else(|| Collection::empty()),
                    );
                    let collection = self_kind.object.as_mut().expect("object was just inserted");

                    match iter.peek() {
                        Some(segment) => {
                            match collection.known_mut().entry(field.into_owned().into()) {
                                Entry::Occupied(entry) => entry.into_mut(),
                                Entry::Vacant(entry) => entry.insert(Kind::null()),
                            }
                        }
                        None => {
                            collection
                                .known_mut()
                                .insert(field.into_owned().into(), kind);
                            return;
                        }
                    }
                }
                BorrowedSegment::Index(mut index) => {
                    // array insertion converts the value to an array, so remove all other types
                    *self_kind = Kind::array(
                        self_kind
                            .array
                            .clone()
                            .unwrap_or_else(|| Collection::empty()),
                    );
                    let collection = self_kind.array.as_mut().expect("array was just inserted");

                    if index < 0 {
                        let largest_known_index =
                            collection.known().keys().map(|i| i.to_usize()).max();
                        // the minimum size of the resulting array
                        let len_required = -index as usize;

                        if let Some(unknown) = collection.unknown() {
                            let unknown_kind = unknown.to_kind();

                            // the array may be larger, but this is the largest we can prove the array is from the type information
                            let min_length = largest_known_index.map(|i| i + 1).unwrap_or(0);

                            if len_required > min_length {
                                // We can't prove the array is large enough, so "holes" may be created
                                // which set the value to null.
                                // Holes are inserted to the front, which shifts everything to the right.
                                // We don't know the exact number of holes, but can determine an upper bound
                                let max_shifts = len_required - min_length;

                                // The number of possible shifts is 0 ..= max_shifts.
                                // Each shift will be calculated independently and merged into the collection.
                                // A shift of 0 is the original collection, so that is skipped
                                for shift_count in 1..=max_shifts {
                                    let mut shifted_collection = collection.clone();
                                    // clear all known values and replace with new ones. (in-place shift can overwrite)
                                    shifted_collection.known_mut().clear();

                                    // add the "null" from holes. Index 0 is handled below
                                    for i in 1..shift_count {
                                        shifted_collection
                                            .known_mut()
                                            .insert(i.into(), Kind::null());
                                    }

                                    // Index 0 is always the inserted value if shifts are happening
                                    let mut item = Kind::null();
                                    item.insert(&iter.clone().collect::<Vec<_>>(), kind.clone());
                                    shifted_collection.known_mut().insert(0.into(), item);

                                    // shift known values by the exact "shift_count"
                                    for (i, i_kind) in collection.known() {
                                        shifted_collection
                                            .known_mut()
                                            .insert(*i + shift_count, i_kind.clone());
                                    }

                                    // add this shift count as another possible type definition
                                    collection.merge(shifted_collection, false);
                                }
                            }

                            // We can prove the positive index won't be less than "min_index"
                            let min_index = (min_length as isize + index).max(0) as usize;

                            // sanity check: if holes are added to the type, min_index must be 0
                            debug_assert!(min_index == 0 || min_length >= len_required);

                            // indices less than the minimum possible index won't change.
                            // Apply the current "unknown" to indices that don't have an explicit known
                            // since the "unknown" is about to change
                            for i in 0..min_index {
                                if !collection.known().contains_key(&i.into()) {
                                    collection
                                        .known_mut()
                                        .insert(i.into(), unknown_kind.clone());
                                }
                            }
                            for (i, i_kind) in collection.known_mut() {
                                // This index might be set by the insertion, add the insertion type to the existing type
                                if i.to_usize() >= min_index {
                                    let mut kind_with_insertion = i_kind.clone();
                                    let remaining_path_segments = iter.clone().collect::<Vec<_>>();
                                    kind_with_insertion
                                        .insert(&remaining_path_segments, kind.clone());
                                    i_kind.merge_keep(kind_with_insertion, false);
                                }
                            }

                            let mut unknown_kind_with_insertion = unknown_kind.clone();
                            let remaining_path_segments = iter.clone().collect::<Vec<_>>();
                            unknown_kind_with_insertion
                                .insert(&remaining_path_segments, kind.clone());
                            let mut new_unknown_kind = unknown_kind;
                            new_unknown_kind.merge_keep(unknown_kind_with_insertion, false);
                            collection.set_unknown(new_unknown_kind);

                            return;
                        } else {
                            // If there is no unknown, the exact position of the negative index can be determined
                            if collection.unknown().is_none() {
                                let exact_array_len = largest_known_index
                                    .map(|max_index| max_index + 1)
                                    .unwrap_or(0);

                                if len_required > exact_array_len {
                                    // fill in holes from extending to fit a negative index
                                    for i in exact_array_len..len_required {
                                        // there is no unknown, so the exact type "null" can be inserted
                                        collection.known_mut().insert(i.into(), Kind::null());
                                    }
                                }
                                index += (len_required as isize).max(exact_array_len as isize);
                            }
                        }
                    }

                    debug_assert!(index >= 0, "all negative cases have been handled");
                    let index = index as usize;

                    match iter.peek() {
                        Some(segment) => match collection.known_mut().entry(index.into()) {
                            Entry::Occupied(entry) => entry.into_mut(),
                            Entry::Vacant(entry) => entry.insert(Kind::null()),
                        },
                        None => {
                            collection.known_mut().insert(index.into(), kind);

                            // add "null" to all holes, adding it to the "unknown" if it exists
                            let hole_type = collection
                                .unknown()
                                .map(|x| x.to_kind())
                                .unwrap_or(Kind::never())
                                .or_null();

                            for i in 0..index {
                                if !collection.known_mut().contains_key(&i.into()) {
                                    collection.known_mut().insert(i.into(), hole_type.clone());
                                }
                            }
                            return;
                        }
                    }
                }
                BorrowedSegment::CoalesceField(field) => {
                    // TODO: This can be improved once "undefined" is a type
                    //   https://github.com/vectordotdev/vector/issues/13459

                    let remaining_segments = iter
                        .clone()
                        .skip_while(|segment| matches!(segment, BorrowedSegment::CoalesceField(_)))
                        // next segment must be a coalesce end, which is skipped
                        .skip(1)
                        .collect::<Vec<_>>();

                    // we don't know for sure if this coalesce will succeed, so the insertion is merged with the original value
                    let mut maybe_inserted_kind = self_kind.clone();
                    maybe_inserted_kind.insert(
                        path!(&field.into_owned()).concat(&remaining_segments),
                        kind.clone(),
                    );
                    self_kind.merge_keep(maybe_inserted_kind, false);
                    self_kind
                }
                BorrowedSegment::CoalesceEnd(field) => {
                    // TODO: This can be improved once "undefined" is a type
                    //   https://github.com/vectordotdev/vector/issues/13459

                    let remaining_segments = iter.clone().collect::<Vec<_>>();

                    // we don't know for sure if this coalesce will succeed, so the insertion is merged with the original value
                    let mut maybe_inserted_kind = self_kind.clone();
                    maybe_inserted_kind.insert(
                        path!(&field.into_owned()).concat(&remaining_segments),
                        kind.clone(),
                    );
                    self_kind.merge_keep(maybe_inserted_kind, false);
                    return;
                }
                BorrowedSegment::Invalid => return,
            };
        }
        *self_kind = kind;
    }
}

#[cfg(test)]
mod tests {
    use lookup::lookup_v2::{parse_path, OwnedPath};
    use lookup::owned_path;
    use std::collections::HashMap;

    use lookup::LookupBuf;

    use super::*;
    use crate::kind::Collection;

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_insert() {
        struct TestCase {
            this: Kind,
            path: OwnedPath,
            kind: Kind,
            expected: Kind,
        }

        for (
            title,
            TestCase {
                mut this,
                path,
                kind,
                expected,
            },
        ) in HashMap::from([
            (
                "root insert",
                TestCase {
                    this: Kind::bytes(),
                    path: owned_path!(),
                    kind: Kind::integer(),
                    expected: Kind::integer(),
                },
            ),
            (
                "root insert object",
                TestCase {
                    this: Kind::bytes(),
                    path: owned_path!(),
                    kind: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "empty object insert field",
                TestCase {
                    this: Kind::object(Collection::empty()),
                    path: owned_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "non-empty object insert field",
                TestCase {
                    this: Kind::object(BTreeMap::from([("b".into(), Kind::bytes())])),
                    path: owned_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([
                        ("a".into(), Kind::integer()),
                        ("b".into(), Kind::bytes()),
                    ])),
                },
            ),
            (
                "object overwrite field",
                TestCase {
                    this: Kind::object(BTreeMap::from([("a".into(), Kind::bytes())])),
                    path: owned_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "set array index on empty array",
                TestCase {
                    this: Kind::array(Collection::empty()),
                    path: owned_path!(0),
                    kind: Kind::integer(),
                    expected: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                },
            ),
            (
                "set array index past the end without unknown",
                TestCase {
                    this: Kind::array(Collection::empty()),
                    path: owned_path!(1),
                    kind: Kind::integer(),
                    expected: Kind::array(BTreeMap::from([
                        (0.into(), Kind::null()),
                        (1.into(), Kind::integer()),
                    ])),
                },
            ),
            (
                "set array index past the end with unknown",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_path!(1),
                    kind: Kind::integer(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(1.into(), Kind::integer())]))
                            .with_unknown(Kind::integer()),
                    ),
                },
            ),
            (
                "set array index past the end with null unknown",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::null())),
                    path: owned_path!(1),
                    kind: Kind::integer(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(1.into(), Kind::integer())]))
                            .with_unknown(Kind::null()),
                    ),
                },
            ),
            (
                "set field on non-object",
                TestCase {
                    this: Kind::integer(),
                    path: owned_path!("a"),
                    kind: Kind::integer(),
                    expected: Kind::object(BTreeMap::from([("a".into(), Kind::integer())])),
                },
            ),
            (
                "set array index on non-array",
                TestCase {
                    this: Kind::integer(),
                    path: owned_path!(0),
                    kind: Kind::integer(),
                    expected: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                },
            ),
            (
                "set negative array index (no unknown)",
                TestCase {
                    this: Kind::array(BTreeMap::from([
                        (0.into(), Kind::integer()),
                        (1.into(), Kind::integer()),
                    ])),
                    path: owned_path!(-1),
                    kind: Kind::bytes(),
                    expected: Kind::array(BTreeMap::from([
                        (0.into(), Kind::integer()),
                        (1.into(), Kind::bytes()),
                    ])),
                },
            ),
            (
                "set negative array index past the end (no unknown)",
                TestCase {
                    this: Kind::array(BTreeMap::from([(0.into(), Kind::integer())])),
                    path: owned_path!(-2),
                    kind: Kind::bytes(),
                    expected: Kind::array(BTreeMap::from([
                        (0.into(), Kind::bytes()),
                        (1.into(), Kind::null()),
                    ])),
                },
            ),
            (
                "set negative array index size 1 unknown array",
                TestCase {
                    this: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::integer())]))
                            .with_unknown(Kind::integer()),
                    ),
                    path: owned_path!(-1),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::bytes().or_integer())]))
                            .with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index empty unknown array",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_path!(-1),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::empty().with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index empty unknown array (2)",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_path!(-2),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::empty().with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index unknown array",
                TestCase {
                    this: Kind::array(
                        Collection::from(BTreeMap::from([(1.into(), Kind::float())]))
                            .with_unknown(Kind::integer()),
                    ),
                    path: owned_path!(-3),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (1.into(), Kind::float().or_bytes().or_null().or_integer()),
                            (2.into(), Kind::float().or_bytes().or_null().or_integer()),
                        ]))
                        .with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index unknown array no holes",
                TestCase {
                    this: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::float()),
                            (1.into(), Kind::float()),
                            (2.into(), Kind::float()),
                        ]))
                        .with_unknown(Kind::integer()),
                    ),
                    path: owned_path!(-3),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([
                            (0.into(), Kind::float().or_bytes()),
                            (1.into(), Kind::float().or_bytes()),
                            (2.into(), Kind::float().or_bytes()),
                        ]))
                        .with_unknown(Kind::integer().or_bytes().or_null()),
                    ),
                },
            ),
            (
                "set negative array index on non-array",
                TestCase {
                    this: Kind::integer(),
                    path: owned_path!(-3),
                    kind: Kind::bytes(),
                    expected: Kind::array(Collection::from(BTreeMap::from([
                        (0.into(), Kind::bytes()),
                        (1.into(), Kind::null()),
                        (2.into(), Kind::null()),
                    ]))),
                },
            ),
            (
                "set nested negative array index on unknown array",
                TestCase {
                    this: Kind::array(Collection::empty().with_unknown(Kind::integer())),
                    path: owned_path!(-3, "foo"),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::empty().with_unknown(
                            Kind::integer()
                                .or_null()
                                .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                        ),
                    ),
                },
            ),
            (
                "set nested negative array index on unknown array (no holes)",
                TestCase {
                    this: Kind::array(
                        Collection::from(BTreeMap::from([(0.into(), Kind::integer())]))
                            .with_unknown(Kind::integer()),
                    ),
                    path: owned_path!(-1, "foo"),
                    kind: Kind::bytes(),
                    expected: Kind::array(
                        Collection::from(BTreeMap::from([(
                            0.into(),
                            Kind::integer()
                                .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                        )]))
                        .with_unknown(
                            Kind::integer()
                                .or_null()
                                .or_object(BTreeMap::from([("foo".into(), Kind::bytes())])),
                        ),
                    ),
                },
            ),
            (
                "coalesce empty object",
                TestCase {
                    this: Kind::object(Collection::empty()),
                    path: parse_path(".(a|b)"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::bytes().or_null()),
                        ("b".into(), Kind::bytes().or_null()),
                    ]))),
                },
            ),
            (
                "coalesce first exists",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([(
                        "a".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_path(".(a|b)"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_bytes()),
                        ("b".into(), Kind::bytes().or_null()),
                    ]))),
                },
            ),
            (
                "coalesce second exists",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([(
                        "b".into(),
                        Kind::integer(),
                    )]))),
                    path: parse_path(".(a|b)"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::bytes().or_null()),
                        ("b".into(), Kind::integer().or_bytes()),
                    ]))),
                },
            ),
            (
                "coalesce both exist",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer()),
                        ("b".into(), Kind::integer()),
                    ]))),
                    path: parse_path(".(a|b)"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        ("a".into(), Kind::integer().or_bytes()),
                        ("b".into(), Kind::integer().or_bytes()),
                    ]))),
                },
            ),
            (
                "coalesce nested",
                TestCase {
                    this: Kind::object(Collection::from(BTreeMap::from([]))),
                    path: parse_path(".(a|b).x"),
                    kind: Kind::bytes(),
                    expected: Kind::object(Collection::from(BTreeMap::from([
                        (
                            "a".into(),
                            Kind::object(BTreeMap::from([("x".into(), Kind::bytes())])).or_null(),
                        ),
                        (
                            "b".into(),
                            Kind::object(BTreeMap::from([("x".into(), Kind::bytes())])).or_null(),
                        ),
                    ]))),
                },
            ),
        ]) {
            this.insert(&path, kind);
            assert_eq!(this, expected, "{}", title);
        }
    }

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
