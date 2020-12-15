use crate::event::Segment;
use crate::{
    event::{timestamp_to_string, Lookup, LookupBuf, SegmentBuf},
    Result,
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use serde::{Deserialize, Serialize, Serializer};
use std::iter::FromIterator;
use std::{
    collections::{BTreeMap, HashMap},
    convert::{TryFrom, TryInto},
    fmt::Debug,
};
use toml::value::Value as TomlValue;

// The ordering of these fields, **particularly timestamps and bytes** is very important as serde's
// untagged enum parser handes it in order.
#[derive(PartialEq, Debug, Clone, Deserialize, is_enum_variant)]
#[serde(untagged)]
pub enum Value {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
    Bytes(Bytes),
    Map(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Null,
}

impl Value {
    #[instrument(level = "trace")]
    pub fn as_integer(&self) -> &i64 {
        match self {
            Value::Integer(ref i) => i,
            _ => panic!("Tried to call `Value::as_integer` on a non-integer value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_integer_mut(&mut self) -> &mut i64 {
        match self {
            Value::Integer(ref mut i) => i,
            _ => panic!("Tried to call `Value::as_integer` on a non-integer value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_float(&self) -> &f64 {
        match self {
            Value::Float(ref f) => f,
            _ => panic!("Tried to call `Value::as_float` on a non-float value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_float_mut(&mut self) -> &mut f64 {
        match self {
            Value::Float(ref mut f) => f,
            _ => panic!("Tried to call `Value::as_float` on a non-float value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_bool(&self) -> &bool {
        match self {
            Value::Boolean(ref b) => b,
            _ => panic!("Tried to call `Value::as_bool` on a non-bool value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_bool_mut(&mut self) -> &mut bool {
        match self {
            Value::Boolean(ref mut b) => b,
            _ => panic!("Tried to call `Value::as_bool` on a non-bool value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_timestamp(&self) -> &DateTime<Utc> {
        match self {
            Value::Timestamp(ref t) => t,
            _ => panic!("Tried to call `Value::as_timestamp` on a non-timestamp value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_timestamp_mut(&mut self) -> &mut DateTime<Utc> {
        match self {
            Value::Timestamp(ref mut t) => t,
            _ => panic!("Tried to call `Value::as_timestamp` on a non-timestamp value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_bytes(&self) -> &Bytes {
        match self {
            Value::Bytes(ref b) => b,
            _ => panic!("Tried to call `Value::as_bytes` on a non-bytes value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_bytes_mut(&mut self) -> &mut Bytes {
        match self {
            Value::Bytes(ref mut b) => b,
            _ => panic!("Tried to call `Value::as_bytes` on a non-bytes value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_map(&self) -> &BTreeMap<String, Value> {
        match self {
            Value::Map(ref m) => m,
            _ => panic!("Tried to call `Value::as_map` on a non-map value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_map_mut(&mut self) -> &mut BTreeMap<String, Value> {
        match self {
            Value::Map(ref mut m) => m,
            _ => panic!("Tried to call `Value::as_map` on a non-map value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_array(&self) -> &Vec<Value> {
        match self {
            Value::Array(ref a) => a,
            _ => panic!("Tried to call `Value::as_array` on a non-array value."),
        }
    }

    #[instrument(level = "trace")]
    pub fn as_array_mut(&mut self) -> &mut Vec<Value> {
        match self {
            Value::Array(ref mut a) => a,
            _ => panic!("Tried to call `Value::as_array` on a non-array value."),
        }
    }

    /// Return if the node is a leaf (meaning it has no children) or not.
    ///
    /// This is notably useful for things like influxdb logs where we list only leaves.
    #[instrument(level = "trace")]
    pub fn is_leaf<'a>(&'a self) -> bool {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => true,
            Value::Map(_) => false,
            Value::Array(_) => false,
        }
    }

    /// Insert a value at a given lookup.
    pub fn insert(
        &mut self,
        lookup: LookupBuf,
        value: impl Into<Value> + Debug,
    ) -> Result<Option<Value>> {
        let mut working_lookup: LookupBuf = lookup.into();
        let span = trace_span!("insert", lookup = %working_lookup);
        let _guard = span.enter();

        let this_segment = working_lookup.pop_front();
        match (this_segment, self) {
            // We've met an end and found our value.
            (None, item) => {
                let mut value = value.into();
                core::mem::swap(&mut value, item);
                trace!("Inserted");
                Ok(Some(value))
            },
            // This is just not allowed!
            (_, Value::Boolean(_))
            | (_, Value::Bytes(_))
            | (_, Value::Timestamp(_))
            | (_, Value::Float(_))
            | (_, Value::Integer(_)) => Err("Cannot insert value nested inside a primitive field.".into()),
            // Descend into a coalesce
            (Some(SegmentBuf::Coalesce(sub_segments)), sub_value) => {
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = LookupBuf::try_from(sub_segment)?;
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the insert.
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    if !sub_value.contains(&lookup) {
                        trace!(option = %lookup, "Found coalesce option.");
                        needle = Some(lookup);
                        break;
                    } else {
                        trace!(option = %lookup, "Did not find coalesce option.");
                    }
                }
                match needle {
                    Some(needle) => {
                        sub_value.insert(needle, value)
                    },
                    None => Ok(None),
                }
            },
            // Descend into a map
            (Some(SegmentBuf::Field { ref name, .. }), Value::Map(map)) => {
                trace!(field = %name, "Seeking into map.");
                let next_value = match working_lookup.get(0) {
                    Some(SegmentBuf::Index(next_len)) => Value::Array(Vec::with_capacity(*next_len)),
                    Some(SegmentBuf::Field { .. }) => Value::Map(Default::default()),
                    Some(SegmentBuf::Coalesce(set)) => {
                        let mut cursor_set = set;
                        loop {
                            match cursor_set.get(0).and_then(|v| v.get(0)) {
                                None => return Err("Met 0 length coalesce sub segment.".into()),
                                Some(SegmentBuf::Field { .. }) => break {
                                    trace!("Preparing inner map.");
                                    Value::Map(Default::default())
                                },
                                Some(SegmentBuf::Index(_)) => break {
                                    trace!("Preparing inner array.");
                                    Value::Array(Vec::with_capacity(0))
                                },
                                Some(SegmentBuf::Coalesce(set)) => cursor_set = &set,
                            }
                        }
                    }
                    None => {
                        trace!(field = %name, "Inserted.");
                        return Ok(map.insert(name.clone(), value.into()))
                    }
                };
                map.entry(name.clone())
                    .or_insert_with(|| {
                        trace!(key = ?name, "Pushing required next value onto map.");
                        next_value
                    })
                    .insert(working_lookup, value)
            }
            (Some(SegmentBuf::Index(_)), Value::Map(_)) => {
                trace!("Mismatched index trying to access map.");
                Ok(None)
            },
            // Descend into an array
            (Some(SegmentBuf::Index(i)), Value::Array(array)) => {
                match array.get_mut(i) {
                    Some(inner) => {
                        trace!(index = ?i, "Seeking into array.");
                        inner.insert(working_lookup, value)
                    },
                    None => {
                        trace!(lenth = ?i, "Array too small, resizing array to fit.");
                        // Fill the vector to the index.
                        array.resize(i, Value::Null);
                        let mut retval = Ok(None);
                        let next_val = match working_lookup.get(0) {
                            Some(SegmentBuf::Index(next_len)) => {
                                let mut inner = Value::Array(Vec::with_capacity(*next_len));
                                trace!("Preparing inner array.");
                                retval = inner.insert(working_lookup, value);
                                inner
                            },
                            Some(SegmentBuf::Field { .. }) => {
                                let mut inner = Value::Map(Default::default());
                                trace!("Preparing inner map.");
                                retval = inner.insert(working_lookup, value);
                                inner
                            },
                            Some(SegmentBuf::Coalesce(set)) => {
                                let mut cursor_set = set;
                                loop {
                                    match cursor_set.get(0).and_then(|v| v.get(0)) {
                                        None => return Err("Met 0 length coalesce sub segment.".into()),
                                        Some(SegmentBuf::Field { .. }) => break {
                                            let mut inner = Value::Map(Default::default());
                                            trace!("Preparing inner map.");
                                            retval = inner.insert(working_lookup, value);
                                            inner
                                        },
                                        Some(SegmentBuf::Index(_)) => break {
                                            let mut inner = Value::Array(Vec::with_capacity(0));
                                            trace!("Preparing inner array.");
                                            retval = inner.insert(working_lookup, value);
                                            inner
                                        },
                                        Some(SegmentBuf::Coalesce(set)) => cursor_set = set,
                                    }
                                }
                            },
                            None => value.into(),
                        };
                        trace!(?next_val, "Setting index to value.");
                        array.push(next_val);
                        retval
                    },
                }
            },
            (Some(SegmentBuf::Field { .. }), Value::Array(_)) => {
                trace!("Mismatched field trying to access array.");
                Ok(None)
            },
            // This situation is surprisingly common due to how nulls full sparse vectors.
            (Some(segment), val) if val == &mut Value::Null => {
                let retval;
                let this_val = match segment {
                    SegmentBuf::Index(_) => {
                        let mut inner = Value::Array(Vec::with_capacity(0));
                        trace!("Preparing inner array.");
                        working_lookup.push_front(segment);
                        retval = inner.insert(working_lookup, value);
                        inner
                    },
                    SegmentBuf::Field { .. } => {
                        let mut inner = Value::Map(Default::default());
                        trace!("Preparing inner map.");
                        working_lookup.push_front(segment);
                        retval = inner.insert(working_lookup, value);
                        inner
                    },
                    SegmentBuf::Coalesce(set) => {
                        let mut cursor_set = &set;
                        loop {
                            match cursor_set.get(0).and_then(|v| v.get(0)) {
                                None => return Err("Met 0 length coalesce sub segment.".into()),
                                Some(SegmentBuf::Field { .. }) => break {
                                    let mut inner = Value::Map(Default::default());
                                    trace!("Preparing inner map.");
                                    retval = inner.insert(working_lookup, value);
                                    inner
                                },
                                Some(SegmentBuf::Index(_)) => break {
                                    let mut inner = Value::Array(Vec::with_capacity(0));
                                    trace!("Preparing inner array.");
                                    retval = inner.insert(working_lookup, value);
                                    inner
                                },
                                Some(SegmentBuf::Coalesce(set)) => cursor_set = &set,
                            }
                        }
                    },
                };
                trace!(val = ?this_val, "Setting previously existing null to value.");
                *val = this_val;
                retval
            },
            (Some(_), Value::Null) => unreachable!("This is covered by the above case."),
        }
    }

    /// Remove a value that exists at a given lookup.
    ///
    /// Setting `prune` to true will also remove the entries of maps and arrays that are emptied.
    #[instrument(level = "trace", skip(self, lookup))]
    pub fn remove<'a>(
        &mut self,
        lookup: impl Into<Lookup<'a>> + Debug,
        prune: bool,
    ) -> Result<Option<Value>> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("insert", lookup = %working_lookup);
        let _guard = span.enter();


        let this_segment = working_lookup.pop_front();
        let mut is_empty = false;

        let retval = match (this_segment, &mut *self) {
            // We've met an end without finding a value. (Terminus nodes on arrays/maps detected prior)
            (None, _item) => {
                trace!("Found nothing to remove.");
                Ok(None)
            },
            // This is just not allowed!
            (_, Value::Boolean(_))
            | (_, Value::Bytes(_))
            | (_, Value::Timestamp(_))
            | (_, Value::Float(_))
            | (_, Value::Integer(_))
            | (_, Value::Null) => Err("Cannot remove value nested inside a primitive field.".into()),
            // Descend into a coalesce
            (Some(Segment::Coalesce(sub_segments)), value) => {
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = Lookup::try_from(sub_segment)?;
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the removal.
                    if value.contains(lookup.clone()) {
                        trace!(option = %lookup, "Found coalesce option.");
                        needle = Some(lookup);
                        break;
                    } else {
                        trace!(option = %lookup, "Did not find coalesce option.");
                    }
                }
                match needle {
                    Some(needle) => {
                        value.remove(needle, prune)
                    },
                    None => Ok(None),
                }
            }
            // Descend into a map
            (Some(Segment::Field { ref name, .. }), Value::Map(map)) => {
                if working_lookup.len() == 0 {
                    trace!(field = ?name, "Removing from map.");
                    Ok(map.remove(*name))
                } else {
                    trace!(field = ?name, "Descending into map.");
                    let retval = match map.get_mut(*name) {
                        Some(inner) => inner.remove(working_lookup.clone(), prune),
                        None => Ok(None),
                    };
                    if map.is_empty() {
                        is_empty = true
                    }
                    retval
                }
            }
            (Some(Segment::Index(_)), Value::Map(_)) => Ok(None),
            // Descend into an array
            (Some(Segment::Index(i)), Value::Array(array)) => {
                if working_lookup.len() == 0 {
                    trace!(index = ?i, "Removing from array.");
                    // We don't **actually** want to remove the index, we just want to swap it with a null.
                    if let Some(value) = array.get_mut(i) {
                        let mut holder = Value::Null;
                        core::mem::swap(value, &mut holder);
                        Ok(Some(holder))
                    } else {
                        Ok(None)
                    }
                } else {
                    trace!(index = ?i, "Descending into array.");
                    let retval = match array.get_mut(i) {
                        Some(inner) => inner.remove(working_lookup.clone(), prune),
                        None => Ok(None),
                    };
                    if array.is_empty() {
                        is_empty = true
                    }
                    retval
                }
            }
            (Some(Segment::Field { .. }), Value::Array(_)) => Ok(None),
        };

        if prune && is_empty {
            *self = Value::Null;
        }

        retval
    }

    /// Get an immutable borrow of the value by lookup.
    #[instrument(level = "trace", skip(self, lookup))]
    pub fn get<'a>(
        &self,
        lookup: impl Into<Lookup<'a>> + Debug,
    ) -> Result<Option<&Value>> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("get", lookup = %working_lookup);
        let _guard = span.enter();

        let this_segment = working_lookup.pop_front();
        match (this_segment, self) {
            // We've met an end and found our value.
            (None, item) => {
                trace!(?item, "Found.");
                Ok(Some(item))
            },
            // Descend into a coalesce
            (Some(Segment::Coalesce(sub_segments)), value) => {
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = Lookup::try_from(sub_segment)?;
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the get.
                    if value.contains(lookup.clone()) {
                        trace!(option = %lookup, "Found coalesce option.");
                        needle = Some(lookup);
                        break;
                    } else {
                        trace!(option = %lookup, "Did not find coalesce option.");
                    }
                }
                match needle {
                    Some(needle) => {
                        trace!(?needle, "Getting inside coalesce option.");
                        value.get(needle)
                    },
                    None => Ok(None),
                }
            }
            // Descend into a map
            (Some(Segment::Field { ref name, .. }), Value::Map(map)) => {
                trace!(field = %name, "Descending into map.");
                match map.get(*name) {
                    Some(inner) => inner.get(working_lookup.clone()),
                    None => {
                        trace!("Found nothing.");
                        Ok(None)
                    },
                }
            }
            (Some(Segment::Index(_)), Value::Map(_)) => {
                trace!("Mismatched index trying to access map.");
                Ok(None)
            },
            // Descend into an array
            (Some(Segment::Index(i)), Value::Array(array)) => {
                trace!(index = %i, "Descending into array.");
                match array.get(i) {
                    Some(inner) => inner.get(working_lookup.clone()),
                    None => {
                        trace!("Found nothing.");
                        Ok(None)
                    },
                }
            },
            (Some(Segment::Field { .. }), Value::Array(_)) => {
                trace!("Mismatched field trying to access array.");
                Ok(None)
            },
            // This is just not allowed!
            (Some(_s), Value::Boolean(_))
            | (Some(_s), Value::Bytes(_))
            | (Some(_s), Value::Timestamp(_))
            | (Some(_s), Value::Float(_))
            | (Some(_s), Value::Integer(_))
            | (Some(_s), Value::Null) => {
                trace!("Mismatched primitive field while trying to use segment.");
                Ok(None)
            },
        }
    }

    /// Get a mutable borrow of the value by lookup.
    pub fn get_mut<'a>(
        &mut self,
        lookup: impl Into<Lookup<'a>> + Debug,
    ) -> Result<Option<&mut Value>> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("get_mut", lookup = %working_lookup);
        let _guard = span.enter();

        let this_segment = working_lookup.pop_front();
        match (this_segment, self) {
            // We've met an end and found our value.
            (None, item) => {
                trace!(?item, "Found.");
                Ok(Some(item))
            },
            // This is just not allowed!
            (_, Value::Boolean(_))
            | (_, Value::Bytes(_))
            | (_, Value::Timestamp(_))
            | (_, Value::Float(_))
            | (_, Value::Integer(_))
            | (_, Value::Null) => unimplemented!(),
            // Descend into a coalesce
            (Some(Segment::Coalesce(sub_segments)), value) => {
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = Lookup::try_from(sub_segment)?;
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the get.
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    if value.contains(lookup.clone()) {
                        trace!(option = %lookup, "Found coalesce option.");
                        needle = Some(lookup);
                        break;
                    } else {
                        trace!(option = %lookup, "Did not find coalesce option.");
                    }
                }
                match needle {
                    Some(needle) => {
                        trace!(?needle, "Getting inside coalesce option.");
                        value.get_mut(needle)
                    },
                    None => Ok(None),
                }
            }
            // Descend into a map
            (Some(Segment::Field { ref name, .. }), Value::Map(map)) => {
                trace!(field = %name, "Seeking into map.");
                match map.get_mut(*name) {
                    Some(inner) => inner.get_mut(working_lookup.clone()),
                    None => Ok(None),
                }
            }
            (Some(Segment::Index(_)), Value::Map(_)) => {
                trace!("Mismatched index trying to access map.");
                Ok(None)
            },
            // Descend into an array
            (Some(Segment::Index(i)), Value::Array(array)) => {
                trace!(index = %i, "Seeking into array.");
                match array.get_mut(i) {
                    Some(inner) => inner.get_mut(working_lookup.clone()),
                    None => Ok(None),
                }
            }
            (Some(Segment::Field { .. }), Value::Array(_)) => {
                trace!("Mismatched field trying to access array.");
                Ok(None)
            }
        }
    }

    /// Get an immutable borrow of the given value by lookup.
    #[instrument(level = "trace", skip(self))]
    pub fn contains<'a>(&self, lookup: impl Into<Lookup<'a>> + Debug) -> bool {
        self.get(lookup.into()).unwrap_or(None).is_some()
    }

    /// Produce an iterator over all 'nodes' in the graph of this value.
    ///
    /// This includes leaf nodes as well as intermediaries.
    ///
    /// If provided a `prefix`, it will always produce with that prefix included, and all nodes
    /// will be prefixed with that lookup.
    #[instrument(level = "trace", skip(self, prefix, only_leaves))]
    pub fn lookups<'a>(
        &'a self,
        prefix: Option<Lookup<'a>>,
        only_leaves: bool,
    ) -> Box<dyn Iterator<Item = Lookup<'a>> + 'a> {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => Box::new(prefix.into_iter().inspect(|v| {
                trace!(prefix = ?v, "Enqueuing leaf for iteration.");
            })),
            Value::Map(m) => {
                trace!(prefix = ?prefix, "Enqueuing for iteration, may have children.");
                let this = prefix.clone().into_iter();
                let children = m
                    .iter()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k),
                            |mut l| {
                                l.push_back(Segment::from(k.as_str()));
                                l
                            },
                        );
                        trace!(lookup = ?lookup, "Seeking lookups inside non-leaf element.");
                        v.lookups(Some(lookup), only_leaves)
                    })
                    .flatten();

                if only_leaves {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
            Value::Array(a) => {
                trace!(prefix = ?prefix, "Enqueuing for iteration, may have children.");
                let this = prefix.clone().into_iter();
                let children = a
                    .iter()
                    .enumerate()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k),
                            |mut l| {
                                l.push_back(Segment::index(k));
                                l
                            },
                        );
                        trace!(lookup = ?lookup, "Seeking lookups inside non-leaf element.");
                        v.lookups(Some(lookup), only_leaves)
                    })
                    .flatten();

                if only_leaves {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
        }
    }

    /// Produce an iterator over all 'nodes' in the graph of this value.
    ///
    /// This includes leaf nodes as well as intermediaries.
    ///
    /// If provided a `prefix`, it will always produce with that prefix included, and all nodes
    /// will be prefixed with that lookup.
    #[instrument(level = "trace", skip(self, prefix, only_leaves))]
    pub fn pairs<'a>(
        &'a self,
        prefix: Option<Lookup<'a>>,
        only_leaves: bool,
    ) -> Box<dyn Iterator<Item = (Lookup<'a>, &'a Value)> + 'a> {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => Box::new(
                prefix
                    .map(move |v| {
                        trace!(prefix = ?v, "Enqueuing leaf for iteration.");
                        (v, self)
                    })
                    .into_iter(),
            ),
            Value::Map(m) => {
                trace!(prefix = ?prefix, "Enqueuing for iteration, may have children.");
                let this = prefix.clone().map(|v| (v, self)).into_iter();
                let children = m
                    .iter()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k),
                            |mut l| {
                                l.push_back(Segment::from(k.as_str()));
                                l
                            },
                        );
                        trace!(lookup = ?lookup, "Seeking lookups inside non-leaf element.");
                        v.pairs(Some(lookup), only_leaves)
                    })
                    .flatten();

                if only_leaves {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
            Value::Array(a) => {
                trace!(prefix = ?prefix, "Enqueuing for iteration, may have children.");
                let this = prefix.clone().map(|v| (v, self)).into_iter();
                let children = a
                    .iter()
                    .enumerate()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k),
                            |mut l| {
                                l.push_back(Segment::index(k));
                                l
                            },
                        );
                        trace!(lookup = ?lookup, "Seeking lookups inside non-leaf element.");
                        v.pairs(Some(lookup), only_leaves)
                    })
                    .flatten();

                if only_leaves {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
        }
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            Value::Integer(i) => serializer.serialize_i64(*i),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::Boolean(b) => serializer.serialize_bool(*b),
            Value::Bytes(_) | Value::Timestamp(_) => {
                serializer.serialize_str(&self.to_string_lossy())
            }
            Value::Map(m) => serializer.collect_map(m),
            Value::Array(a) => serializer.collect_seq(a),
            Value::Null => serializer.serialize_none(),
        }
    }
}

impl From<Bytes> for Value {
    fn from(bytes: Bytes) -> Self {
        Value::Bytes(bytes)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(set: Vec<T>) -> Self {
        Value::from_iter(set.into_iter().map(|v| v.into()))
    }
}

impl From<String> for Value {
    fn from(string: String) -> Self {
        Value::Bytes(string.into())
    }
}

impl TryFrom<TomlValue> for Value {
    type Error = crate::Error;

    fn try_from(toml: TomlValue) -> crate::Result<Self> {
        Ok(match toml {
            TomlValue::String(s) => Self::from(s),
            TomlValue::Integer(i) => Self::from(i),
            TomlValue::Array(a) => Self::from(
                a.into_iter()
                    .map(Value::try_from)
                    .collect::<Result<Vec<_>>>()?,
            ),
            TomlValue::Table(t) => Self::from(
                t.into_iter()
                    .map(|(k, v)| Value::try_from(v).map(|v| (k, v)))
                    .collect::<Result<BTreeMap<_, _>>>()?,
            ),
            TomlValue::Datetime(dt) => Self::from(dt.to_string().parse::<DateTime<Utc>>()?),
            TomlValue::Boolean(b) => Self::from(b),
            TomlValue::Float(f) => Self::from(f),
        })
    }
}

// We only enable this in testing for convenience, since `"foo"` is a `&str`.
// In normal operation, it's better to let the caller decide where to clone and when, rather than
// hiding this from them.
#[cfg(test)]
impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Bytes(Vec::from(s.as_bytes()).into())
    }
}

impl From<DateTime<Utc>> for Value {
    fn from(timestamp: DateTime<Utc>) -> Self {
        Value::Timestamp(timestamp)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        match value {
            None => Value::Null,
            Some(v) => v.into(),
        }
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::Float(f64::from(value))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Map(value)
    }
}

impl From<HashMap<String, Value>> for Value {
    fn from(value: HashMap<String, Value>) -> Self {
        Value::from_iter(value.into_iter())
    }
}

impl FromIterator<Value> for Value {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> Self {
        Value::Array(iter.into_iter().collect::<Vec<Value>>())
    }
}

impl FromIterator<(String, Value)> for Value {
    fn from_iter<I: IntoIterator<Item = (String, Value)>>(iter: I) -> Self {
        Value::Map(iter.into_iter().collect::<BTreeMap<String, Value>>())
    }
}

macro_rules! impl_valuekind_from_integer {
    ($t:ty) => {
        impl From<$t> for Value {
            fn from(value: $t) -> Self {
                Value::Integer(value as i64)
            }
        }
    };
}

impl_valuekind_from_integer!(i64);
impl_valuekind_from_integer!(i32);
impl_valuekind_from_integer!(i16);
impl_valuekind_from_integer!(i8);
impl_valuekind_from_integer!(isize);

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Boolean(value)
    }
}

impl From<serde_json::Value> for Value {
    fn from(json_value: serde_json::Value) -> Self {
        match json_value {
            serde_json::Value::Bool(b) => Value::Boolean(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::Bytes(n.to_string().into())
                }
            }
            serde_json::Value::String(s) => Value::Bytes(Bytes::from(s)),
            serde_json::Value::Object(obj) => Value::Map(
                obj.into_iter()
                    .map(|(key, value)| (key, Value::from(value)))
                    .collect(),
            ),
            serde_json::Value::Array(arr) => {
                Value::Array(arr.into_iter().map(Value::from).collect())
            }
            serde_json::Value::Null => Value::Null,
        }
    }
}

impl From<serde_json::Map<String, serde_json::Value>> for Value {
    fn from(json_value: serde_json::Map<String, serde_json::Value>) -> Self {
        Value::Map(
            json_value
                .into_iter()
                .map(|(key, value)| (key, Value::from(value)))
                .collect(),
        )
    }
}

impl TryInto<serde_json::Value> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<serde_json::Value, Self::Error> {
        match self {
            Value::Boolean(v) => Ok(serde_json::Value::from(v)),
            Value::Integer(v) => Ok(serde_json::Value::from(v)),
            Value::Float(v) => Ok(serde_json::Value::from(v)),
            Value::Bytes(v) => Ok(serde_json::Value::from(String::from_utf8(v.to_vec())?)),
            Value::Map(v) => Ok(serde_json::to_value(v)?),
            Value::Array(v) => Ok(serde_json::to_value(v)?),
            Value::Null => Ok(serde_json::Value::Null),
            Value::Timestamp(v) => Ok(serde_json::Value::from(timestamp_to_string(&v))),
        }
    }
}

impl TryInto<bool> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<bool, Self::Error> {
        match self {
            Value::Boolean(v) => Ok(v),
            _ => Err(
                "Tried to call `Value::try_into` to get a bool from a type that was not a bool."
                    .into(),
            ),
        }
    }
}

impl TryInto<Bytes> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<Bytes, Self::Error> {
        match self {
            Value::Bytes(v) => Ok(v),
            _ => Err(
                "Tried to call `Value::try_into` to get a Bytes from a type that was not a Bytes."
                    .into(),
            ),
        }
    }
}

impl TryInto<f64> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<f64, Self::Error> {
        match self {
            Value::Float(v) => Ok(v),
            _ => Err(
                "Tried to call `Value::try_into` to get a f64 from a type that was not a f64."
                    .into(),
            ),
        }
    }
}

impl TryInto<i64> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<i64, Self::Error> {
        match self {
            Value::Integer(v) => Ok(v),
            _ => Err(
                "Tried to call `Value::try_into` to get a i64 from a type that was not a i64."
                    .into(),
            ),
        }
    }
}

impl TryInto<BTreeMap<String, Value>> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<BTreeMap<String, Value>, Self::Error> {
        match self {
            Value::Map(v) => Ok(v),
            _ => Err("Tried to call `Value::try_into` to get a BTreeMap<String, Value> from a type that was not a BTreeMap<String, Value>.".into())
        }
    }
}

impl TryInto<Vec<Value>> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<Vec<Value>, Self::Error> {
        match self {
            Value::Array(v) => Ok(v),
            _ => Err("Tried to call `Value::try_into` to get a Vec<Value> from a type that was not a Vec<Value>.".into())
        }
    }
}

impl TryInto<DateTime<Utc>> for Value {
    type Error = crate::Error;

    fn try_into(self) -> std::result::Result<DateTime<Utc>, Self::Error> {
        match self {
            Value::Timestamp(v) => Ok(v),
            _ => Err("Tried to call `Value::try_into` to get a DateTime from a type that was not a DateTime.".into())
        }
    }
}

impl From<remap::Value> for Value {
    fn from(v: remap::Value) -> Self {
        use remap::Value::*;

        match v {
            Bytes(v) => Value::Bytes(v),
            Integer(v) => Value::Integer(v),
            Float(v) => Value::Float(v),
            Boolean(v) => Value::Boolean(v),
            Map(v) => Value::Map(v.into_iter().map(|(k, v)| (k, v.into())).collect()),
            Array(v) => Value::Array(v.into_iter().map(Into::into).collect()),
            Timestamp(v) => Value::Timestamp(v),
            Regex(v) => Value::Bytes(bytes::Bytes::copy_from_slice(v.to_string().as_bytes())),
            Null => Value::Null,
        }
    }
}

impl From<Value> for remap::Value {
    fn from(v: Value) -> Self {
        use remap::Value::*;

        match v {
            Value::Bytes(v) => Bytes(v),
            Value::Integer(v) => Integer(v),
            Value::Float(v) => Float(v),
            Value::Boolean(v) => Boolean(v),
            Value::Map(v) => Map(v.into_iter().map(|(k, v)| (k, v.into())).collect()),
            Value::Array(v) => Array(v.into_iter().map(Into::into).collect()),
            Value::Timestamp(v) => Timestamp(v),
            Value::Null => Null,
        }
    }
}

impl Value {
    // TODO: return Cow
    pub fn to_string_lossy(&self) -> String {
        match self {
            Value::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Value::Timestamp(timestamp) => timestamp_to_string(timestamp),
            Value::Integer(num) => format!("{}", num),
            Value::Float(num) => format!("{}", num),
            Value::Boolean(b) => format!("{}", b),
            Value::Map(map) => serde_json::to_string(map).expect("Cannot serialize map"),
            Value::Array(arr) => serde_json::to_string(arr).expect("Cannot serialize array"),
            Value::Null => "<null>".to_string(),
        }
    }

    pub fn clone_into_bytes(&self) -> Bytes {
        match self {
            Value::Bytes(bytes) => bytes.clone(), // cloning a Bytes is cheap
            Value::Timestamp(timestamp) => Bytes::from(timestamp_to_string(timestamp)),
            Value::Integer(num) => Bytes::from(format!("{}", num)),
            Value::Float(num) => Bytes::from(format!("{}", num)),
            Value::Boolean(b) => Bytes::from(format!("{}", b)),
            Value::Map(map) => Bytes::from(serde_json::to_vec(map).expect("Cannot serialize map")),
            Value::Array(arr) => {
                Bytes::from(serde_json::to_vec(arr).expect("Cannot serialize array"))
            }
            Value::Null => Bytes::from("<null>"),
        }
    }

    pub fn kind(&self) -> &str {
        match self {
            Value::Bytes(_) => "string",
            Value::Timestamp(_) => "timestamp",
            Value::Integer(_) => "integer",
            Value::Float(_) => "float",
            Value::Boolean(_) => "boolean",
            Value::Map(_) => "map",
            Value::Array(_) => "array",
            Value::Null => "null",
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{fs, io::Read, path::Path};

    fn parse_artifact(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
        let mut test_file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) => return Err(e),
        };

        let mut buf = Vec::new();
        test_file.read_to_end(&mut buf)?;

        Ok(buf)
    }

    mod insert_get_remove {
        use super::*;

        #[test]
        fn single_field() {
            crate::test_util::trace_init();
            let mut value = Value::from(BTreeMap::default());
            let key = "root";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_map()[key], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));
        }

        #[test]
        fn nested_field() {
            crate::test_util::trace_init();
            let mut value = Value::from(BTreeMap::default());
            let key = "root.doot";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_map()["root"].as_map()["doot"], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));
        }

        #[test]
        fn single_index() {
            crate::test_util::trace_init();
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_array()[0], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));
        }

        #[test]
        fn nested_index() {
            crate::test_util::trace_init();
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0][0]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_array()[0].as_array()[0], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));
        }

        #[test]
        fn field_index() {
            crate::test_util::trace_init();
            let mut value = Value::from(BTreeMap::default());
            let key = "root[0]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_map()["root"].as_array()[0], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));
        }

        #[test]
        fn index_field() {
            crate::test_util::trace_init();
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0].boot";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_array()[0].as_map()["boot"], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));
        }

        #[test]
        fn nested_index_field() {
            crate::test_util::trace_init();
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0][0].boot";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_array()[0].as_array()[0].as_map()["boot"], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));
        }

        #[test]
        fn field_with_nested_index_field() {
            crate::test_util::trace_init();
            let mut value = Value::from(BTreeMap::default());
            let key = "root[0][0].boot";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_map()["root"].as_array()[0].as_array()[0].as_map()["boot"], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));
        }

        #[test]
        fn coalesced_index() {
            crate::test_util::trace_init();
            let mut value = Value::from(Vec::<Value>::default());
            let key = "([0] | [1])";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_array()[0], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));
        }

        #[test]
        fn coalesced_index_with_tail() {
            crate::test_util::trace_init();
            let mut value = Value::from(Vec::<Value>::default());
            let key = "([0] | [1]).bloop";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None); // Duplicated on purpose.
            assert_eq!(value.as_array()[0].as_map()["bloop"], marker);
            assert_eq!(value.as_array()[1].as_map()["bloop"], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone()));

            assert_eq!(value.as_array()[1].as_map()["bloop"], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker)); // Duplicated on purpose.
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker)); // Duplicated on purpose.
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker.clone())); // Duplicated on purpose.
        }
    }

    // This test iterates over the `tests/data/fixtures/value` folder and:
    //   * Ensures the parsed folder name matches the parsed type of the `Value`.
    //   * Ensures the `serde_json::Value` to `vector::Value` conversions are harmless. (Think UTF-8 errors)
    //
    // Basically: This test makes sure we aren't mutilating any content users might be sending.
    #[test]
    fn json_value_to_vector_value_to_json_value() {
        crate::test_util::trace_init();
        const FIXTURE_ROOT: &str = "tests/data/fixtures/value";

        tracing::trace!(?FIXTURE_ROOT, "Opening");
        std::fs::read_dir(FIXTURE_ROOT).unwrap().for_each(|type_dir| match type_dir {
            Ok(type_name) => {
                let path = type_name.path();
                tracing::trace!(?path, "Opening");
                std::fs::read_dir(path).unwrap().for_each(|fixture_file| match fixture_file {
                    Ok(fixture_file) => {
                        let path = fixture_file.path();
                        let buf = parse_artifact(&path).unwrap();

                        let serde_value: serde_json::Value = serde_json::from_slice(&*buf).unwrap();
                        let vector_value = Value::from(serde_value.clone());

                        // Validate type
                        let expected_type = type_name.path().file_name().unwrap().to_string_lossy().to_string();
                        assert!(match &*expected_type {
                            "boolean" => vector_value.is_boolean(),
                            "integer" => vector_value.is_integer(),
                            "bytes" => vector_value.is_bytes(),
                            "array" => vector_value.is_array(),
                            "map" => vector_value.is_map(),
                            "null" => vector_value.is_null(),
                            _ => unreachable!("You need to add a new type handler here."),
                        }, "Typecheck failure. Wanted {}, got {:?}.", expected_type, vector_value);

                        let serde_value_again: serde_json::Value = vector_value.clone().try_into().unwrap();

                        tracing::trace!(?path, ?serde_value, ?vector_value, ?serde_value_again, "Asserting equal.");
                        assert_eq!(
                            serde_value,
                            serde_value_again
                        );
                    },
                    _ => panic!("This test should never read Err'ing test fixtures."),
                });
            },
            _ => panic!("This test should never read Err'ing type folders."),
        })
    }
}
