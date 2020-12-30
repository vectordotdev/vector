#![allow(clippy::needless_collect)]

pub mod lua;
#[cfg(test)]
mod test;

use crate::{event::*, lookup::*};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::{
    collections::{btree_map::Entry, BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    fmt::Debug,
    iter::FromIterator,
};
use tracing::{debug, error, instrument, trace, trace_span};

/// A map of [`crate::event::Value`].
///
/// The inside of an [`Event::Log`](crate::event::Event) variant of [`crate::event::Event`].
///
/// This type supports being interacted with like a regular old
/// [`BTreeMap`](std::collections::BTreeMap), or with special (unowned) [`crate::event::Lookup`] and
/// (owned) [`crate::event::LookupBuf`] types.
///
/// Transparently, as a normal [`BTreeMap`](std::collections::BTreeMap):
///
/// ```rust
/// use shared::{event::*, lookup::*};
/// let mut event = LogEvent::default();
/// event.insert(String::from("foo"), 1);
/// assert!(event.contains("foo"));
/// assert_eq!(event.get("foo"), Some(&Value::from(1)));
/// ```
///
/// Using remap-style lookups:
///
/// ```rust
/// use shared::{event::*, lookup::*};
/// let mut event = LogEvent::default();
/// let lookup = LookupBuf::from_str("foo[0].(bar | bat)").unwrap();
/// event.insert(lookup.clone(), 1);
/// assert!(event.contains(&lookup));
/// assert_eq!(event.get(&lookup), Some(&Value::from(1)));
/// ```
///
/// It's possible to access the inner [`BTreeMap`](std::collections::BTreeMap):
///
/// ```rust
/// use shared::{event::*, lookup::*};
/// let mut event = LogEvent::default();
/// event.insert(String::from("foo"), 1);
///
/// use std::collections::BTreeMap;
/// let _inner: &BTreeMap<_, _> = event.inner();
/// let _inner: &mut BTreeMap<_, _> = event.inner_mut();
/// let inner: BTreeMap<_, _> = event.take();
///
/// let event = LogEvent::from(inner);
/// ```
///
/// There exists a `log_event` macro you may also utilize to create this type:
///
/// ```rust
/// use shared::{log_event, event::*, lookup::*};
/// let event = log_event! {
///     "foo" => 1,
///     LookupBuf::from_str("bar.baz").unwrap() => 2,
/// }.into_log();
/// assert!(event.contains("foo"));
/// assert!(event.contains(Lookup::from_str("foo").unwrap()));
/// ```
#[derive(PartialEq, Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogEvent {
    #[serde(flatten)]
    fields: BTreeMap<String, Value>,
}

impl LogEvent {
    /// Get an immutable borrow of the given value by lookup.
    ///
    /// ```rust
    /// use shared::{log_event, event::*, lookup::*};
    /// let plain_key = "foo";
    /// let lookup_key = LookupBuf::from_str("bar.baz").unwrap();
    /// let event = log_event! {
    ///     plain_key => 1,
    ///     lookup_key.clone() => 2,
    /// }.into_log();
    /// assert_eq!(event.get(plain_key), Some(&Value::from(1)));
    /// assert_eq!(event.get(&lookup_key), Some(&Value::from(2)));
    /// ```
    pub fn get<'a>(&self, lookup: impl Into<Lookup<'a>> + Debug) -> Option<&Value> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("get", lookup = %working_lookup);
        let _guard = span.enter();

        // The first step should always be a field.
        let this_segment = working_lookup.pop_front().unwrap();
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        match this_segment {
            Segment::Coalesce(sub_segments) => {
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = Lookup::try_from(sub_segment).ok()?;
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the removal.
                    if self.contains(lookup.clone()) {
                        trace!(option = %lookup, "Found coalesce option.");
                        needle = Some(lookup);
                        break;
                    } else {
                        trace!(option = %lookup, "Did not find coalesce option.");
                    }
                }
                match needle {
                    Some(needle) => self.get(needle),
                    None => None,
                }
            }
            Segment::Field {
                name,
                requires_quoting: _,
            } => {
                if working_lookup.len() == 0 {
                    // Terminus: We **must** get something here, else we truly have nothing.
                    trace!(field = %name, "Getting from root.");
                    self.fields.get(name)
                } else {
                    trace!(field = %name, "Descending into map.");
                    match self.fields.get(name) {
                        Some(v) => v.get(working_lookup).unwrap_or_else(|error| {
                            debug!("{:?}", error);
                            None
                        }),
                        None => None,
                    }
                }
            }
            // In this case, the user has passed us an invariant.
            Segment::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                None
            }
        }
    }

    /// Get a mutable borrow of the value by lookup.
    ///
    /// ```rust
    /// use shared::{log_event, event::*, lookup::*};
    /// let plain_key = "foo";
    /// let lookup_key = LookupBuf::from_str("bar.baz").unwrap();
    /// let mut event = log_event! {
    ///     plain_key => 1,
    ///     lookup_key.clone() => 2,
    /// }.into_log();
    /// assert_eq!(event.get_mut(plain_key), Some(&mut Value::from(1)));
    /// assert_eq!(event.get_mut(&lookup_key), Some(&mut Value::from(2)));
    /// ```
    pub fn get_mut<'a>(&mut self, lookup: impl Into<Lookup<'a>> + Debug) -> Option<&mut Value> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("get_mut", lookup = %working_lookup);
        let _guard = span.enter();

        // The first step should always be a field.
        let this_segment = working_lookup.pop_front().unwrap();
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        match this_segment {
            Segment::Coalesce(sub_segments) => {
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = Lookup::try_from(sub_segment).ok()?;
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the removal.
                    if self.contains(lookup.clone()) {
                        trace!(option = %lookup, "Found coalesce option.");
                        needle = Some(lookup);
                        break;
                    } else {
                        trace!(option = %lookup, "Did not find coalesce option.");
                    }
                }
                match needle {
                    Some(needle) => self.get_mut(needle),
                    None => None,
                }
            }
            Segment::Field {
                name,
                requires_quoting: _,
            } => {
                if working_lookup.len() == 0 {
                    // Terminus: We **must** insert here or abort.
                    trace!(field = %name, "Getting from root.");
                    self.fields.get_mut(name)
                } else {
                    trace!(field = %name, "Descending into map.");
                    match self.fields.get_mut(name) {
                        Some(v) => v.get_mut(working_lookup).unwrap_or_else(|error| {
                            debug!("{:?}", error);
                            None
                        }),
                        None => None,
                    }
                }
            }
            // In this case, the user has passed us an invariant.
            Segment::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                None
            }
        }
    }

    /// Determine if the log event contains a value at a given lookup.
    ///
    /// ```rust
    /// use shared::{log_event, event::*, lookup::*};
    /// let plain_key = "foo";
    /// let lookup_key = LookupBuf::from_str("bar.baz").unwrap();
    /// let mut event = log_event! {
    ///     plain_key => 1,
    ///     lookup_key.clone() => 2,
    /// }.into_log();
    /// assert!(event.contains(plain_key));
    /// assert!(event.contains(&lookup_key));
    /// ```
    pub fn contains<'a>(&self, lookup: impl Into<Lookup<'a>> + Debug) -> bool {
        let working_lookup = lookup.into();
        let span = trace_span!("contains", lookup = %working_lookup);
        let _guard = span.enter();

        self.get(working_lookup).is_some()
    }

    /// Insert a value at a given lookup, returning any old value that exists.
    ///
    /// ```rust
    /// use shared::{log_event, event::*, lookup::*};
    /// let plain_key = "foo";
    /// let lookup_key = LookupBuf::from_str("bar.baz").unwrap();
    /// let mut event = log_event! {
    ///     plain_key => 1,
    ///     lookup_key.clone() => 2,
    /// }.into_log();
    /// assert_eq!(event.insert(plain_key, i64::MAX), Some(Value::from(1)));
    /// assert_eq!(event.insert(lookup_key.clone(), i64::MAX), Some(Value::from(2)));
    /// ```
    pub fn insert(
        &mut self,
        lookup: impl Into<LookupBuf>,
        value: impl Into<Value> + Debug,
    ) -> Option<Value> {
        let mut working_lookup: LookupBuf = lookup.into();
        let span = trace_span!("insert", lookup = %working_lookup);
        let _guard = span.enter();

        // The first step should always be a field.
        let this_segment = working_lookup.pop_front().unwrap();
        // This is good, since the first step into a LogEvent will also be a field.

        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        match this_segment {
            SegmentBuf::Coalesce(sub_segments) => {
                trace!("Seeking first match of coalesce.");
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = LookupBuf::try_from(sub_segment).ok()?;
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the removal.
                    if !self.contains(&lookup) {
                        trace!(option = %lookup, "Found coalesce option.");
                        needle = Some(lookup);
                        break;
                    } else {
                        trace!(option = %lookup, "Did not find coalesce option.");
                    }
                }
                match needle {
                    Some(needle) => self.insert(needle, value),
                    None => None,
                }
            }
            SegmentBuf::Field {
                name,
                requires_quoting: _,
            } => {
                let next_value = match working_lookup.get(0) {
                    Some(SegmentBuf::Index(_)) => Value::Array(Vec::with_capacity(0)),
                    Some(SegmentBuf::Field { .. }) => Value::Map(Default::default()),
                    Some(SegmentBuf::Coalesce(set)) => {
                        let mut cursor_set = set;
                        loop {
                            match cursor_set.get(0).and_then(|v| v.get(0)) {
                                None => return None,
                                Some(SegmentBuf::Field { .. }) => {
                                    break Value::Map(Default::default())
                                }
                                Some(SegmentBuf::Index(i)) => {
                                    break Value::Array(Vec::with_capacity(*i))
                                }
                                Some(SegmentBuf::Coalesce(set)) => cursor_set = &set,
                            }
                        }
                    }
                    None => {
                        trace!(field = %name, "Inserting into root of event.");
                        return self.fields.insert(name, value.into());
                    }
                };
                trace!(field = %name, "Seeking into map.");
                let entry = self.fields.entry(name.clone()).or_insert_with(|| {
                    trace!(field = %name, "Inserting at leaf.");
                    next_value
                });
                let outcome = entry.insert(working_lookup, value);
                match outcome {
                    Ok(v) => v,
                    Err(EventError::PrimitiveDescent {
                        primitive_at,
                        original_target,
                        original_value: Some(original_value),
                    }) => {
                        trace!(%primitive_at, %original_target, "Encountered descent into a primitive.");
                        // When we find a primitive descent, we overwrite it.
                        match entry.remove(&primitive_at, true) {
                            Err(EventError::RemovingSelf) => {
                                self.fields.remove(&name);
                                trace!(%primitive_at, "Removed primitive, trying again.");
                                let mut target = LookupBuf::from(name);
                                target.extend(original_target);
                                self.insert(target, original_value)
                            }
                            _ => entry
                                .insert(original_target, original_value)
                                .map_err(|error| {
                                    debug!("{:?}", error);
                                    error
                                })
                                .unwrap_or(Option::<Value>::None),
                        }
                    }
                    Err(error) => {
                        debug!("{:?}", error);
                        None
                    }
                }
            }
            // In this case, the user has passed us an invariant.
            SegmentBuf::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                None
            }
        }
    }

    /// Remove a value that exists at a given lookup.
    ///
    /// Setting `prune` to true will also remove the entries of maps and arrays that are emptied.
    ///
    /// ```rust
    /// use shared::{log_event, event::*, lookup::*};
    /// let plain_key = "foo";
    /// let lookup_key = LookupBuf::from_str("bar.baz.slam").unwrap();
    /// let mut event = log_event! {
    ///     plain_key => 1,
    ///     lookup_key.clone() => 2,
    /// }.into_log();
    /// assert_eq!(event.remove(plain_key, true), Some(Value::from(1)));
    /// assert_eq!(event.remove(&lookup_key, true), Some(Value::from(2)));
    /// // Since we pruned, observe how `bar` is also removed because `prune` is set:
    /// assert!(!event.contains("bar.baz"));
    /// ```
    pub fn remove<'lookup>(
        &mut self,
        lookup: impl Into<Lookup<'lookup>> + Debug,
        prune: bool,
    ) -> Option<Value> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("remove", lookup = %working_lookup);
        let _guard = span.enter();

        // The first step should always be a field.
        let this_segment = working_lookup.pop_front().unwrap();
        // This step largely exists so that we can make `cursor` a `Value` right off the bat.
        // We couldn't go like `let cursor = Value::from(self.fields)` since that'd take the value.
        match this_segment {
            Segment::Coalesce(sub_segments) => {
                trace!("Seeking first match of coalesce.");
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = Lookup::try_from(sub_segment).ok()?;
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the removal.
                    if self.contains(lookup.clone()) {
                        trace!(option = %lookup, "Found coalesce option.");
                        needle = Some(lookup);
                        break;
                    } else {
                        trace!(option = %lookup, "Did not find coalesce option.");
                    }
                }
                match needle {
                    Some(needle) => self.remove(needle, prune),
                    None => None,
                }
            }
            Segment::Field {
                name,
                requires_quoting: _,
            } => {
                if working_lookup.len() == 0 {
                    // Terminus: We **must** insert here or abort.
                    trace!(field = %name, "Getting from root.");
                    // Do not need to prune, it's already a root value.
                    self.fields.remove(name)
                } else {
                    trace!(field = %name, "Seeking into map.");
                    let retval = match self.fields.get_mut(name) {
                        Some(v) => v.remove(working_lookup, prune).unwrap_or_else(|e| {
                            trace!(?e);
                            None
                        }),
                        None => None,
                    };
                    if let Some(val) = self.fields.get_mut(name) {
                        if val.is_empty() && prune {
                            trace!(is_empty = val.is_empty(), %prune, field = %name, "Pruning.");
                            self.fields.remove(name);
                        } else {
                            trace!(is_empty = val.is_empty(), %prune, field = %name, "Not pruning.");
                        }
                    }
                    retval
                }
            }
            // In this case, the user has passed us an invariant.
            Segment::Index(_) => {
                error!(
                    "Lookups into LogEvents should never start with indexes.\
                        Please report your config."
                );
                None
            }
        }
    }

    /// Iterate over the lookups available in this log event.
    ///
    /// This is notably different than the keys in a map, as this descends into things like arrays
    /// and maps. It also returns those array/map values during iteration.
    ///
    /// ```rust
    /// use shared::{log_event, event::*, lookup::*};
    /// let plain_key = "lick";
    /// let lookup_key = LookupBuf::from_str("vic.stick.slam").unwrap();
    /// let event = log_event! {
    ///     plain_key => 1,
    ///     lookup_key.clone() => 2,
    /// }.into_log();
    /// let mut keys = event.keys(false);
    /// assert_eq!(keys.next(), Some(Lookup::from_str("lick").unwrap()));
    /// assert_eq!(keys.next(), Some(Lookup::from_str("vic").unwrap()));
    /// assert_eq!(keys.next(), Some(Lookup::from_str("vic.stick").unwrap()));
    /// assert_eq!(keys.next(), Some(Lookup::from_str("vic.stick.slam").unwrap()));
    ///
    /// let mut keys = event.keys(true);
    /// assert_eq!(keys.next(), Some(Lookup::from_str("lick").unwrap()));
    /// assert_eq!(keys.next(), Some(Lookup::from_str("vic.stick.slam").unwrap()));
    /// ```
    #[instrument(level = "trace", skip(self, only_leaves))]
    pub fn keys<'a>(&'a self, only_leaves: bool) -> impl Iterator<Item = Lookup<'a>> + 'a {
        self.fields
            .iter()
            .map(move |(k, v)| {
                let lookup = Lookup::from(k);
                v.lookups(Some(lookup), only_leaves)
            })
            .flatten()
    }

    /// Iterate over all lookup/value pairs.
    ///
    /// This is notably different than pairs in a map, as this descends into things like arrays and
    /// maps. It also returns those array/map values during iteration.
    ///
    /// ```rust
    /// use shared::{log_event, event::*, lookup::*};
    /// let plain_key = "lick";
    /// let lookup_key = LookupBuf::from_str("vic.stick.slam").unwrap();
    /// let event = log_event! {
    ///     plain_key => 1,
    ///     lookup_key.clone() => 2,
    /// }.into_log();
    /// let mut keys = event.pairs(false);
    /// assert_eq!(keys.next(), Some((Lookup::from_str("lick").unwrap(), &Value::from(1))));
    /// assert_eq!(keys.next(), Some((Lookup::from_str("vic").unwrap(), &Value::from({
    ///     let mut inner_map = std::collections::BTreeMap::default();
    ///     inner_map.insert(String::from("slam"), Value::from(2));
    ///     let mut map = std::collections::BTreeMap::default();
    ///     map.insert(String::from("stick"), Value::from(inner_map));
    ///     map
    /// }))));
    /// assert_eq!(keys.next(), Some((Lookup::from_str("vic.stick").unwrap(), &Value::from({
    ///     let mut map = std::collections::BTreeMap::default();
    ///     map.insert(String::from("slam"), Value::from(2));
    ///     map
    /// }))));
    /// assert_eq!(keys.next(), Some((Lookup::from_str("vic.stick.slam").unwrap(), &Value::from(2))));
    ///
    /// let mut keys = event.pairs(true);
    /// assert_eq!(keys.next(), Some((Lookup::from_str("lick").unwrap(), &Value::from(1))));
    /// assert_eq!(keys.next(), Some((Lookup::from_str("vic.stick.slam").unwrap(), &Value::from(2))));
    /// ```
    #[instrument(level = "trace", skip(self, only_leaves))]
    pub fn pairs<'a>(&'a self, only_leaves: bool) -> impl Iterator<Item = (Lookup<'a>, &'a Value)> {
        self.fields
            .iter()
            .map(move |(k, v)| {
                let lookup = Lookup::from(k);
                v.pairs(Some(lookup), only_leaves)
            })
            .flatten()
    }

    /// Determine if the log event is empty of fields.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
    /// let event = LogEvent::default();
    /// assert!(event.is_empty());
    /// ```
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Return an entry for the given lookup.
    #[instrument(level = "trace", skip(self, lookup), fields(lookup = %lookup), err)]
    pub fn entry(&mut self, lookup: LookupBuf) -> crate::Result<Entry<String, Value>> {
        trace!("Seeking to entry.");
        let mut walker = lookup.into_iter().enumerate();

        let mut current_pointer = if let Some((
            index,
            SegmentBuf::Field {
                name: segment,
                requires_quoting: _,
            },
        )) = walker.next()
        {
            trace!(%segment, index, "Seeking segment.");
            self.fields.entry(segment)
        } else {
            unreachable!(
                "It is an invariant to have a `Lookup` without a contained `Segment`.\
                `Lookup::is_valid` should catch this during `Lookup` creation, maybe it was not \
                called?."
            );
        };

        for (index, segment) in walker {
            trace!(%segment, index, "Seeking next segment.");
            current_pointer = match (segment, current_pointer) {
                (
                    SegmentBuf::Field {
                        name,
                        requires_quoting: _,
                    },
                    Entry::Occupied(entry),
                ) => match entry.into_mut() {
                    Value::Map(map) => map.entry(name),
                    v => return Err(format!("Looking up field on a non-map value: {:?}", v).into()),
                },
                (
                    SegmentBuf::Field {
                        name,
                        requires_quoting: _,
                    },
                    Entry::Vacant(entry),
                ) => {
                    trace!(segment = %name, index, "Met vacant entry.");
                    return Err(format!(
                        "Tried to step into `{}` of `{}`, but it did not exist.",
                        name,
                        entry.key()
                    )
                    .into());
                }
                _ => return Err("The entry API cannot yet descend into array indices.".into()),
            };
        }
        trace!(entry = ?current_pointer, "Result.");
        Ok(current_pointer)
    }

    /// Returns the entire event as a `Value::Map`.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
    /// let event = LogEvent::default();
    /// assert_eq!(event.take(), std::collections::BTreeMap::default());
    /// ```
    #[instrument(level = "trace", skip(self))]
    pub fn take(self) -> BTreeMap<String, Value> {
        self.fields
    }

    /// Get a borrow of the contained fields.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
    /// let mut event = LogEvent::default();
    /// assert_eq!(event.inner(), &std::collections::BTreeMap::default());
    /// ```
    #[instrument(level = "trace", skip(self))]
    pub fn inner(&self) -> &BTreeMap<String, Value> {
        &self.fields
    }

    /// Get a mutable borrow of the contained fields.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
    /// let mut event = LogEvent::default();
    /// assert_eq!(event.inner_mut(), &mut std::collections::BTreeMap::default());
    /// ```
    #[instrument(level = "trace", skip(self))]
    pub fn inner_mut(&mut self) -> &mut BTreeMap<String, Value> {
        &mut self.fields
    }
}

impl remap_lang::Object for LogEvent {
    fn get(&self, path: &remap_lang::Path) -> Result<Option<remap_lang::Value>, String> {
        if path.is_root() {
            Ok(Some(Value::from(self.inner().clone()).into()))
        } else {
            trace!(path = %path.to_string(), "Converting to LookupBuf.");
            let lookup = LookupBuf::try_from(path).map_err(|e| format!("{}", e))?;
            let val = self.get(&lookup);
            // TODO: This does not need to clone.
            Ok(val.map(Clone::clone).map(Into::into))
        }
    }

    fn remove(&mut self, path: &remap_lang::Path, compact: bool) -> Result<(), String> {
        if path.is_root() {
            let mut value = LogEvent::default();
            std::mem::swap(self, &mut value);
            // TODO: Why does this not return value?
            Ok(())
        } else {
            trace!(path = %path.to_string(), "Converting to LookupBuf.");
            let lookup = LookupBuf::try_from(path).map_err(|e| format!("{}", e))?;
            let _val = self.remove(&lookup, compact);
            // TODO: Why does this not return?
            Ok(())
        }
    }

    fn insert(&mut self, path: &remap_lang::Path, value: remap_lang::Value) -> Result<(), String> {
        let value = Value::from(value);
        if path.is_root() {
            if let Value::Map(mut v) = value {
                std::mem::swap(&mut self.fields, &mut v);
                // TODO: Why does this not return value?
                Ok(())
            } else {
                Err("Cannot insert as root of Event unless it is a map.".into())
            }
        } else {
            trace!(path = %path.to_string(), "Converting to LookupBuf.");
            // TODO: We should not degrade the error to a string here.
            let lookup = LookupBuf::try_from(path).map_err(|e| format!("{}", e))?;
            let _val = self.insert(lookup, value);
            // TODO: Why does this not return?
            Ok(())
        }
    }

    fn paths(&self) -> Result<Vec<remap_lang::Path>, String> {
        // The LogEvent API itself is not able to consistently return `pairs()` and `keys()` including
        // the root, so it's done here, instead.
        let this = Some(Lookup::default());
        let rest = self.keys(true);
        this.into_iter()
            .chain(rest)
            .map(|v| {
                remap_lang::Path::from_str(v.to_string().as_str())
                    // TODO: We should not degrade the error to a string here.
                    .map_err(|v| format!("{:?}", v))
            })
            .collect()
    }
}

impl From<BTreeMap<String, Value>> for LogEvent {
    fn from(map: BTreeMap<String, Value>) -> Self {
        LogEvent { fields: map }
    }
}

impl Into<BTreeMap<String, Value>> for LogEvent {
    fn into(self) -> BTreeMap<String, Value> {
        let Self { fields } = self;
        fields
    }
}

impl From<HashMap<String, Value>> for LogEvent {
    fn from(map: HashMap<String, Value>) -> Self {
        LogEvent {
            fields: map.into_iter().collect(),
        }
    }
}

impl Into<HashMap<String, Value>> for LogEvent {
    fn into(self) -> HashMap<String, Value> {
        self.fields.into_iter().collect()
    }
}

impl TryFrom<serde_json::Value> for LogEvent {
    type Error = crate::Error;

    fn try_from(map: serde_json::Value) -> Result<Self, Self::Error> {
        match map {
            serde_json::Value::Object(fields) => Ok(LogEvent::from(
                fields
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<BTreeMap<_, _>>(),
            )),
            _ => Err(crate::Error::from(
                "Attempted to convert non-Object JSON into a LogEvent.",
            )),
        }
    }
}

impl TryInto<serde_json::Value> for LogEvent {
    type Error = crate::Error;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        let Self { fields } = self;
        Ok(serde_json::to_value(fields)?)
    }
}

impl<'a, V> Extend<(LookupBuf, V)> for LogEvent
where
    V: Into<Value>,
{
    fn extend<I: IntoIterator<Item = (LookupBuf, V)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k, v.into());
        }
    }
}

// Allow converting any kind of appropriate key/value iterator directly into a LogEvent.
impl<'a, V: Into<Value>> FromIterator<(LookupBuf, V)> for LogEvent {
    fn from_iter<T: IntoIterator<Item = (LookupBuf, V)>>(iter: T) -> Self {
        let mut log_event = LogEvent::default();
        log_event.extend(iter);
        log_event
    }
}

/// Converts event into an iterator over top-level key/value pairs.
impl IntoIterator for LogEvent {
    type Item = (String, Value);
    type IntoIter = std::collections::btree_map::IntoIter<String, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.fields.into_iter()
    }
}

impl<T> std::ops::Index<T> for LogEvent
where
    T: Into<Lookup<'static>> + Debug,
{
    type Output = Value;

    fn index(&self, key: T) -> &Value {
        self.get(key).expect("Key not found.")
    }
}

impl<T> std::ops::IndexMut<T> for LogEvent
where
    T: Into<Lookup<'static>> + Debug,
{
    fn index_mut(&mut self, key: T) -> &mut Value {
        self.get_mut(key).expect("Key not found.")
    }
}
