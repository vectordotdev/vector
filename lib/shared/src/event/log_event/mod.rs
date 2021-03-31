#![allow(clippy::needless_collect)]

pub mod lua;
#[cfg(test)]
mod test;

use crate::{event::*, lookup::*};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use std::{
    collections::{btree_map::Entry, BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    fmt::Debug,
    iter::FromIterator,
};
use tracing::{debug, info, instrument, trace, trace_span};

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
/// It's possible to access the inner [`Value`](crate::event::Value):
///
/// ```rust
/// use shared::{event::*, lookup::*};
/// use std::convert::TryFrom;
/// let mut event = LogEvent::default();
/// event.insert(String::from("foo"), 1);
///
/// use std::collections::BTreeMap;
/// let _inner: &Value = event.inner();
/// let _inner: &mut Value = event.inner_mut();
/// let inner: Value = event.take();
///
/// let event = LogEvent::try_from(inner).unwrap();
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
#[derive(PartialEq, Debug, Clone, Derivative, Serialize, Deserialize)]
#[derivative(Default)]
pub struct LogEvent {
    // **IMPORTANT:** Due to numerous legacy reasons this **must** be a Map variant.
    #[serde(flatten)]
    #[derivative(Default(value = "Value::from(BTreeMap::default())"))]
    fields: Value,
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
        let working_lookup = lookup.into();
        let span = trace_span!("get", lookup = %working_lookup);
        let _guard = span.enter();

        self.fields.get(working_lookup).unwrap_or_else(|error| {
            debug!(%error, "Error while getting immutable borrow.");
            None
        })
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
        let working_lookup = lookup.into();
        let span = trace_span!("get_mut", lookup = %working_lookup);
        let _guard = span.enter();

        self.fields.get_mut(working_lookup).unwrap_or_else(|error| {
            debug!(%error, "Error while getting mutable borrow.");
            None
        })
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
        let working_lookup: LookupBuf = lookup.into();
        let span = trace_span!("insert", lookup = %working_lookup);
        let _guard = span.enter();

        let outcome = self.fields.insert(working_lookup, value);
        match outcome {
            Ok(v) => v,
            Err(EventError::PrimitiveDescent {
                primitive_at,
                original_target,
                original_value: Some(original_value),
            }) => {
                trace!(%primitive_at, %original_target, "Encountered descent into a primitive.");
                // When we find a primitive descent, we overwrite it.
                match self.fields.remove(&primitive_at, true) {
                    Err(EventError::RemovingSelf) => {
                        trace!("Must remove self.");
                        let mut val = Value::from(BTreeMap::default());
                        core::mem::swap(&mut self.fields, &mut val);
                        Some(val)
                    },
                    Err(error) => {
                        debug!(%primitive_at, %original_target, %error, "Error while removing primitive.");
                        None
                    }
                    _ => self.fields
                        .insert(original_target, original_value)
                        .unwrap_or_else(|error| {
                            debug!(%primitive_at, %error, "Error while inserting after removing primitive.");
                            Option::<Value>::None
                        }),
                }
            }
            Err(error) => {
                debug!(%error, "Error while inserting.");
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
        let working_lookup = lookup.into();
        let span = trace_span!("remove", lookup = %working_lookup);
        let _guard = span.enter();

        if working_lookup == Lookup::default() {
            info!("Tried to remove lookup `.` from LogEvent. Refusing.");
            return None;
        }

        self.fields
            .remove(working_lookup, prune)
            .unwrap_or_else(|error| {
                debug!(%error, "Error while removing");
                None
            })
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
    /// assert_eq!(keys.next(), Some(Lookup::from_str(".").unwrap()));
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
        self.fields.lookups(None, only_leaves)
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
    ///     lookup_key => 2,
    /// }.into_log();
    /// let mut keys = event.pairs(false);
    /// assert_eq!(keys.next(), Some((Lookup::from_str(".").unwrap(), &Value::from({
    ///     let mut inner_inner_map = std::collections::BTreeMap::default();
    ///     inner_inner_map.insert(String::from("slam"), Value::from(2));
    ///     let mut inner_map = std::collections::BTreeMap::default();
    ///     inner_map.insert(String::from("stick"), Value::from(inner_inner_map));
    ///     let mut map = std::collections::BTreeMap::default();
    ///     map.insert(String::from("vic"), Value::from(inner_map));
    ///     map.insert(String::from("lick"), Value::from(1));
    ///     map
    /// }))));
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
        self.fields.pairs(None, only_leaves)
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
            self.fields.as_map_mut().entry(segment)
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
    /// assert_eq!(event.take(), Value::Map(std::collections::BTreeMap::default()));
    /// ```
    #[instrument(level = "trace", skip(self))]
    pub fn take(self) -> Value {
        self.fields
    }

    /// Get a borrow of the contained fields.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
    /// let mut event = LogEvent::default();
    /// assert_eq!(event.inner(), &Value::Map(std::collections::BTreeMap::default()));
    /// ```
    #[instrument(level = "trace", skip(self))]
    pub fn inner(&self) -> &Value {
        &self.fields
    }

    /// Get a mutable borrow of the contained fields.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
    /// let mut event = LogEvent::default();
    /// assert_eq!(event.inner_mut(), &Value::Map(std::collections::BTreeMap::default()));
    /// ```
    #[instrument(level = "trace", skip(self))]
    pub fn inner_mut(&mut self) -> &mut Value {
        &mut self.fields
    }
}

impl vrl::Target for LogEvent {
    fn get(&self, path: &vrl::Path) -> Result<Option<vrl::Value>, String> {
        if path.is_root() {
            Ok(Some(self.inner().clone().into()))
        } else {
            trace!(path = %path.to_string(), "Converting to LookupBuf.");
            let lookup = LookupBuf::try_from(path).map_err(|e| format!("{}", e))?;
            let val = self.get(&lookup);
            // TODO: This does not need to clone.
            Ok(val.map(Clone::clone).map(Into::into))
        }
    }

    fn remove(
        &mut self,
        path: &vrl::Path,
        compact: bool,
    ) -> Result<Option<vrl::Value>, String> {
        if path.is_root() {
            Ok(Some({
                let mut value = LogEvent::default();
                std::mem::swap(self, &mut value);
                value
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect::<BTreeMap<_, _>>()
                    .into()
            }))
        } else {
            let lookup = LookupBuf::try_from(path).map_err(|e| format!("{}", e))?;
            Ok(self.remove(&lookup, compact).map(Into::into))
        }
    }

    fn insert(&mut self, path: &vrl::Path, value: vrl::Value) -> Result<(), String> {
        let mut value = Value::from(value);
        if path.is_root() {
            if let Value::Map(_) = value {
                std::mem::swap(&mut self.fields, &mut value);
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
}

impl From<BTreeMap<String, Value>> for LogEvent {
    fn from(map: BTreeMap<String, Value>) -> Self {
        LogEvent {
            fields: Value::from(map),
        }
    }
}

impl Into<BTreeMap<String, Value>> for LogEvent {
    fn into(self) -> BTreeMap<String, Value> {
        let Self { fields } = self;
        fields.try_into().expect("Tried to turn a log event which was not a map into a map. This is an invariant, please report it.")
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
        let map: BTreeMap<_, _> = self.into();
        map.into_iter().collect()
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

impl TryFrom<Value> for LogEvent {
    type Error = crate::Error;

    fn try_from(fields: Value) -> Result<Self, Self::Error> {
        match fields {
            Value::Map(_) => Ok(Self { fields }),
            _ => Err(crate::Error::from(
                "Attempted to convert non-Map value into a LogEvent.",
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
        let map: BTreeMap<_, _> = self.fields.try_into().expect("Tried to turn a log event which was not a map into a map. This is an invariant, please report it.");
        map.into_iter()
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
