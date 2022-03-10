//! Contains the main "Value" type for Vector and VRL, as well as helper methods.

mod convert;
mod display;
mod error;
mod path;
mod regex;
mod target;

#[cfg(feature = "api")]
mod api;
#[cfg(feature = "arbitrary")]
mod arbitrary;
#[cfg(feature = "lua")]
mod lua;
#[cfg(feature = "json")]
mod serde;
#[cfg(feature = "toml")]
mod toml;

use std::{
    collections::BTreeMap,
    fmt::Debug,
    hash::{Hash, Hasher},
};

use bytes::{Bytes, BytesMut};
use chrono::{DateTime, SecondsFormat, Utc};
use error::ValueError;
use lookup::lookup_v2::{BorrowedSegment, Path};
use lookup::{Field, FieldBuf, Lookup, LookupBuf, Segment, SegmentBuf};
use ordered_float::NotNan;
use std::result::Result as StdResult;
use tracing::{instrument, trace, trace_span};

pub use crate::value::regex::ValueRegex;

/// A boxed `std::error::Error`.
pub type StdError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// The main value type used in Vector events, and VRL.
#[derive(PartialOrd, Debug, Clone)]
pub enum Value {
    /// Bytes - usually representing a UTF8 String.
    Bytes(Bytes),

    /// Regex.
    /// When used in the context of Vector this is treated identically to Bytes. It has
    /// additional meaning in the context of VRL.
    Regex(ValueRegex),

    /// Integer.
    Integer(i64),

    /// Float - not NaN.
    Float(NotNan<f64>),

    /// Boolean.
    Boolean(bool),

    /// Timetamp (UTC).
    Timestamp(DateTime<Utc>),

    /// Object.
    Object(BTreeMap<String, Value>),

    /// Array.
    Array(Vec<Value>),

    /// Null.
    Null,
}

impl Eq for Value {}

impl PartialEq<Self> for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Array(a), Value::Array(b)) => a.eq(b),
            (Value::Boolean(a), Value::Boolean(b)) => a.eq(b),
            (Value::Bytes(a), Value::Bytes(b)) => a.eq(b),
            (Value::Regex(a), Value::Regex(b)) => a.eq(b),
            (Value::Float(a), Value::Float(b)) => {
                // This compares floats with the following rules:
                // * NaNs compare as equal
                // * Positive and negative infinity are not equal
                // * -0 and +0 are not equal
                // * Floats will compare using truncated portion
                if a.is_sign_negative() == b.is_sign_negative() {
                    if a.is_finite() && b.is_finite() {
                        a.trunc().eq(&b.trunc())
                    } else {
                        a.is_finite() == b.is_finite()
                    }
                } else {
                    false
                }
            }
            (Value::Integer(a), Value::Integer(b)) => a.eq(b),
            (Value::Object(a), Value::Object(b)) => a.eq(b),
            (Value::Null, Value::Null) => true,
            (Value::Timestamp(a), Value::Timestamp(b)) => a.eq(b),
            _ => false,
        }
    }
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Value::Array(v) => {
                v.hash(state);
            }
            Value::Boolean(v) => {
                v.hash(state);
            }
            Value::Bytes(v) => {
                v.hash(state);
            }
            Value::Regex(regex) => {
                regex.as_bytes_slice().hash(state);
            }
            Value::Float(v) => {
                // This hashes floats with the following rules:
                // * NaNs hash as equal (covered by above discriminant hash)
                // * Positive and negative infinity has to different values
                // * -0 and +0 hash to different values
                // * otherwise transmute to u64 and hash
                if v.is_finite() {
                    v.is_sign_negative().hash(state);
                    let trunc: u64 = v.trunc().to_bits();
                    trunc.hash(state);
                } else if !v.is_nan() {
                    v.is_sign_negative().hash(state);
                } //else covered by discriminant hash
            }
            Value::Integer(v) => {
                v.hash(state);
            }
            Value::Object(v) => {
                v.hash(state);
            }
            Value::Null => {
                //covered by discriminant hash
            }
            Value::Timestamp(v) => {
                v.hash(state);
            }
        }
    }
}

impl Value {
    /// Returns a string description of the value type
    pub const fn kind_str(&self) -> &str {
        match self {
            Value::Bytes(_) | Value::Regex(_) => "string",
            Value::Timestamp(_) => "timestamp",
            Value::Integer(_) => "integer",
            Value::Float(_) => "float",
            Value::Boolean(_) => "boolean",
            Value::Object(_) => "map",
            Value::Array(_) => "array",
            Value::Null => "null",
        }
    }

    /// Merges `incoming` value into self.
    ///
    /// Will concatenate `Bytes` and overwrite the rest value kinds.
    pub fn merge(&mut self, incoming: Self) {
        match (self, incoming) {
            (Value::Bytes(self_bytes), Value::Bytes(ref incoming)) => {
                let mut bytes = BytesMut::with_capacity(self_bytes.len() + incoming.len());
                bytes.extend_from_slice(&self_bytes[..]);
                bytes.extend_from_slice(&incoming[..]);
                *self_bytes = bytes.freeze();
            }
            (current, incoming) => *current = incoming,
        }
    }

    /// Return if the node is empty, that is, it is an array or map with no items.
    ///
    /// ```rust
    /// use value::Value;
    /// use std::collections::BTreeMap;
    ///
    /// let val = Value::from(1);
    /// assert_eq!(val.is_empty(), false);
    ///
    /// let mut val = Value::from(Vec::<Value>::default());
    /// assert_eq!(val.is_empty(), true);
    /// val.insert(0, 1);
    /// assert_eq!(val.is_empty(), false);
    /// val.insert(3, 1);
    /// assert_eq!(val.is_empty(), false);
    ///
    /// let mut val = Value::from(BTreeMap::default());
    /// assert_eq!(val.is_empty(), true);
    /// val.insert("foo", 1);
    /// assert_eq!(val.is_empty(), false);
    /// val.insert("bar", 2);
    /// assert_eq!(val.is_empty(), false);
    /// ```
    pub fn is_empty(&self) -> bool {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Regex(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_) => false,
            Value::Null => true,
            Value::Object(v) => v.is_empty(),
            Value::Array(v) => v.is_empty(),
        }
    }

    fn insert_coalesce(
        sub_segments: Vec<FieldBuf>,
        working_lookup: &LookupBuf,
        sub_value: &mut Self,
        value: Self,
    ) -> StdResult<Option<Self>, ValueError> {
        // Creating a needle with a back out of the loop is very important.
        let mut needle = None;
        for sub_segment in sub_segments {
            let mut lookup = LookupBuf::from(sub_segment);
            lookup.extend(working_lookup.clone()); // We need to include the rest of the insert.
                                                   // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                                                   // contains cost extra. It's super unfortunate, hopefully future work can solve this.
            if !sub_value.contains(&lookup) {
                needle = Some(lookup);
                break;
            }
        }
        match needle {
            Some(needle) => sub_value.insert(needle, value),
            None => Ok(None),
        }
    }

    /// Ensures the value is the correct type for the given segment.
    /// An Index needs the value to be an Array, the others need it to be a Map.
    fn correct_type(value: &mut Self, segment: &SegmentBuf) {
        match segment {
            SegmentBuf::Index(next_len) if !matches!(value, Value::Array(_)) => {
                *value = Self::Array(Vec::with_capacity(next_len.abs() as usize));
            }
            SegmentBuf::Field(_) if !matches!(value, Value::Object(_)) => {
                *value = Self::Object(BTreeMap::default());
            }
            SegmentBuf::Coalesce(_set) if !matches!(value, Value::Object(_)) => {
                *value = Self::Object(BTreeMap::default());
            }
            _ => (),
        }
    }

    fn insert_map(
        name: &str,
        requires_quoting: bool,
        mut working_lookup: LookupBuf,
        map: &mut BTreeMap<String, Self>,
        value: Self,
    ) -> StdResult<Option<Self>, ValueError> {
        let next_segment = match working_lookup.get(0) {
            Some(segment) => segment,
            None => {
                return Ok(map.insert(name.to_string(), value));
            }
        };

        map.entry(name.to_string())
            .and_modify(|entry| Self::correct_type(entry, next_segment))
            .or_insert_with(|| {
                // The entry this segment is referring to doesn't exist, so we must push the appropriate type
                // into the value.
                match next_segment {
                    SegmentBuf::Index(next_len) => {
                        Self::Array(Vec::with_capacity(next_len.abs() as usize))
                    }
                    SegmentBuf::Field(_) | SegmentBuf::Coalesce(_) => {
                        Self::Object(BTreeMap::default())
                    }
                }
            })
            .insert(working_lookup, value)
            .map_err(|mut e| {
                if let ValueError::PrimitiveDescent {
                    original_target,
                    primitive_at,
                    original_value: _,
                } = &mut e
                {
                    let segment = SegmentBuf::Field(FieldBuf {
                        name: name.to_string(),
                        requires_quoting,
                    });
                    original_target.push_front(segment.clone());
                    primitive_at.push_front(segment);
                };
                e
            })
    }

    #[allow(clippy::too_many_lines)]
    fn insert_array(
        i: isize,
        mut working_lookup: LookupBuf,
        array: &mut Vec<Self>,
        value: Self,
    ) -> StdResult<Option<Self>, ValueError> {
        let index = if i.is_negative() {
            array.len() as isize + i
        } else {
            i
        };

        let item = if index.is_negative() {
            // A negative index is greater than the length of the array, so we
            // are trying to set an index that doesn't yet exist.
            None
        } else {
            array.get_mut(index as usize)
        };

        if let Some(inner) = item {
            if let Some(next_segment) = working_lookup.get(0) {
                Self::correct_type(inner, next_segment);
            }

            inner.insert(working_lookup, value).map_err(|mut e| {
                if let ValueError::PrimitiveDescent {
                    original_target,
                    primitive_at,
                    original_value: _,
                } = &mut e
                {
                    let segment = SegmentBuf::Index(i);
                    original_target.push_front(segment.clone());
                    primitive_at.push_front(segment);
                };
                e
            })
        } else {
            if i.is_negative() {
                // Resizing for a negative index must resize to the left.
                // Setting x[-4] to true for an array [0,1] must end up with
                // [true, null, 0, 1]
                let abs = i.abs() as usize - 1;
                let len = array.len();

                array.resize(abs, Self::Null);
                array.rotate_right(abs - len);
            } else {
                // Fill the vector to the index.
                array.resize(i as usize, Self::Null);
            }
            let mut retval = Ok(None);
            let next_val = match working_lookup.get(0) {
                Some(SegmentBuf::Index(next_len)) => {
                    let mut inner = Self::Array(Vec::with_capacity(next_len.abs() as usize));
                    retval = inner.insert(working_lookup, value).map_err(|mut e| {
                        if let ValueError::PrimitiveDescent {
                            original_target,
                            primitive_at,
                            original_value: _,
                        } = &mut e
                        {
                            let segment = SegmentBuf::Index(i);
                            original_target.push_front(segment.clone());
                            primitive_at.push_front(segment);
                        };
                        e
                    });
                    inner
                }
                Some(SegmentBuf::Field(FieldBuf {
                    name,
                    requires_quoting,
                })) => {
                    let mut inner = Self::Object(BTreeMap::default());
                    let name = name.clone(); // This is for navigating an ownership issue in the error stack reporting.
                    let requires_quoting = *requires_quoting; // This is for navigating an ownership issue in the error stack reporting.
                    retval = inner.insert(working_lookup, value).map_err(|mut e| {
                        if let ValueError::PrimitiveDescent {
                            original_target,
                            primitive_at,
                            original_value: _,
                        } = &mut e
                        {
                            let segment = SegmentBuf::Field(FieldBuf {
                                name,
                                requires_quoting,
                            });
                            original_target.push_front(segment.clone());
                            primitive_at.push_front(segment);
                        };
                        e
                    });
                    inner
                }
                Some(SegmentBuf::Coalesce(set)) => match set.get(0) {
                    None => return Err(ValueError::EmptyCoalesceSubSegment),
                    Some(_) => {
                        let mut inner = Self::Object(BTreeMap::default());
                        let set = SegmentBuf::Coalesce(set.clone());
                        retval = inner.insert(working_lookup, value).map_err(|mut e| {
                            if let ValueError::PrimitiveDescent {
                                original_target,
                                primitive_at,
                                original_value: _,
                            } = &mut e
                            {
                                original_target.push_front(set.clone());
                                primitive_at.push_front(set.clone());
                            };
                            e
                        });
                        inner
                    }
                },
                None => value,
            };
            array.push(next_val);
            if i.is_negative() {
                // We need to push to the front of the array.
                array.rotate_right(1);
            }
            retval
        }
    }

    /// Insert a value at a given lookup.
    ///
    /// ```rust
    /// use value::Value;
    /// use lookup::Lookup;
    /// use std::collections::BTreeMap;
    ///
    /// let mut inner_map = Value::from(BTreeMap::default());
    /// inner_map.insert("baz", 1);
    ///
    /// let mut map = Value::from(BTreeMap::default());
    /// map.insert("bar", inner_map.clone());
    /// map.insert("star", inner_map.clone());
    ///
    /// assert!(map.contains("bar"));
    /// assert!(map.contains(Lookup::from_str("star.baz").unwrap()));
    /// ```
    #[allow(clippy::missing_errors_doc)]
    pub fn insert(
        &mut self,
        lookup: impl Into<LookupBuf> + Debug,
        value: impl Into<Self> + Debug,
    ) -> StdResult<Option<Self>, ValueError> {
        let mut working_lookup: LookupBuf = lookup.into();
        let value = value.into();
        let span = trace_span!("insert", lookup = %working_lookup);
        let _guard = span.enter();

        let this_segment = working_lookup.pop_front();
        match (this_segment, self) {
            // We've met an end and found our value.
            (None, item) => {
                let mut value = value;
                core::mem::swap(&mut value, item);
                Ok(Some(value))
            }
            // This is just not allowed and should not occur.
            // The top level insert will always be a map (or an array in tests).
            // Then for further descents into the lookup, in the `insert_map` function
            // if the type is one of the following, the field is modified to be a map.
            (
                Some(segment),
                Value::Boolean(_)
                | Value::Bytes(_)
                | Value::Regex(_)
                | Value::Timestamp(_)
                | Value::Float(_)
                | Value::Integer(_)
                | Value::Null,
            ) => {
                trace!("Encountered descent into a primitive.");
                Err(ValueError::PrimitiveDescent {
                    primitive_at: LookupBuf::default(),
                    original_target: {
                        let mut l = LookupBuf::from(segment);
                        l.extend(working_lookup);
                        l
                    },
                    original_value: Some(value),
                })
            }
            // Descend into a coalesce
            (Some(SegmentBuf::Coalesce(sub_segments)), sub_value) => {
                Self::insert_coalesce(sub_segments, &working_lookup, sub_value, value)
            }
            // Descend into a map
            (
                Some(SegmentBuf::Field(FieldBuf {
                    ref name,
                    ref requires_quoting,
                })),
                Value::Object(ref mut map),
            ) => Self::insert_map(name, *requires_quoting, working_lookup, map, value),
            (Some(SegmentBuf::Index(_)), Value::Object(_)) => {
                trace!("Mismatched index trying to access map.");
                Ok(None)
            }
            // Descend into an array
            (Some(SegmentBuf::Index(i)), Value::Array(ref mut array)) => {
                Self::insert_array(i, working_lookup, array, value)
            }
            (Some(SegmentBuf::Field(FieldBuf { .. })), Value::Array(_)) => {
                trace!("Mismatched field trying to access array.");
                Ok(None)
            }
        }
    }

    /// Remove a value that exists at a given lookup.
    ///
    /// Setting `prune` to true will also remove the entries of maps and arrays that are emptied.
    ///
    /// ```rust
    /// use value::Value;
    /// use lookup::Lookup;
    /// use std::collections::BTreeMap;
    ///
    /// let mut inner_map = Value::from(BTreeMap::default());
    /// inner_map.insert("baz", 1);
    ///
    /// let mut map = Value::from(BTreeMap::default());
    /// map.insert("bar", inner_map.clone());
    /// map.insert("star", inner_map.clone());
    ///
    /// assert_eq!(map.remove("bar", true).unwrap(), Some(Value::from(inner_map)));
    ///
    /// let lookup_key = Lookup::from_str("star.baz").unwrap();
    /// assert_eq!(map.remove(lookup_key, true).unwrap(), Some(Value::from(1)));
    /// assert!(!map.contains("star"));
    /// ```
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::missing_errors_doc)]
    pub fn remove<'a>(
        &mut self,
        lookup: impl Into<Lookup<'a>> + Debug,
        prune: bool,
    ) -> StdResult<Option<Self>, ValueError> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("remove", lookup = %working_lookup, %prune);
        let _guard = span.enter();

        let this_segment = working_lookup.pop_front();

        let retval = match (this_segment, &mut *self) {
            // We've met an end without finding a value. (Terminus nodes on arrays/maps detected prior)
            (None, _item) => {
                trace!("Found nothing to remove.");
                Ok(None)
            }
            // This is just not allowed!
            (
                Some(segment),
                Value::Boolean(_)
                | Value::Bytes(_)
                | Value::Regex(_)
                | Value::Timestamp(_)
                | Value::Float(_)
                | Value::Integer(_)
                | Value::Null,
            ) => {
                if working_lookup.is_empty() {
                    trace!("Cannot remove self. Caller must remove.");
                    Err(ValueError::RemovingSelf)
                } else {
                    trace!("Encountered descent into a primitive.");
                    Err(ValueError::PrimitiveDescent {
                        primitive_at: LookupBuf::default(),
                        original_target: {
                            let mut l = LookupBuf::from(segment.clone().into_buf());
                            l.extend(working_lookup.into_buf());
                            l
                        },
                        original_value: None,
                    })
                }
            }
            // Descend into a coalesce
            (Some(Segment::Coalesce(sub_segments)), value) => {
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = Lookup::from(sub_segment);
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the removal.
                    if value.contains(lookup.clone()) {
                        needle = Some(lookup);
                        break;
                    }
                }
                match needle {
                    Some(needle) => value.remove(needle, prune),
                    None => Ok(None),
                }
            }
            // Descend into a map
            (Some(Segment::Field(Field { name, .. })), Value::Object(map)) => {
                if working_lookup.is_empty() {
                    Ok(map.remove(name))
                } else {
                    let mut inner_is_empty = false;
                    let retval = match map.get_mut(name) {
                        Some(inner) => {
                            let ret = inner.remove(working_lookup.clone(), prune);
                            if inner.is_empty() {
                                inner_is_empty = true;
                            };
                            ret
                        }
                        None => Ok(None),
                    };
                    if inner_is_empty && prune {
                        map.remove(name);
                    }
                    retval
                }
            }
            (Some(Segment::Index(_)), Value::Object(_))
            | (Some(Segment::Field { .. }), Value::Array(_)) => Ok(None),
            // Descend into an array
            (Some(Segment::Index(i)), Value::Array(array)) => {
                let index = if i.is_negative() {
                    if i.abs() > array.len() as isize {
                        // The index is before the start of the array.
                        return Ok(None);
                    }
                    (array.len() as isize + i) as usize
                } else {
                    i as usize
                };

                if working_lookup.is_empty() {
                    // We don't **actually** want to remove the index, we just
                    // want to swap it with a null.
                    if array.len() > index {
                        Ok(Some(array.remove(index)))
                    } else {
                        Ok(None)
                    }
                } else {
                    let mut inner_is_empty = false;
                    let retval = match array.get_mut(index) {
                        Some(inner) => {
                            let ret = inner.remove(working_lookup.clone(), prune);
                            if inner.is_empty() {
                                inner_is_empty = true;
                            }
                            ret
                        }
                        None => Ok(None),
                    };
                    if inner_is_empty && prune {
                        array.remove(index);
                    }
                    retval
                }
            }
        };

        retval
    }

    /// Get an immutable borrow of the value by lookup.
    ///
    /// ```rust
    /// use value::Value;
    /// use lookup::Lookup;
    /// use std::collections::BTreeMap;
    ///
    /// let mut inner_map = Value::from(BTreeMap::default());
    /// inner_map.insert("baz", 1);
    ///
    /// let mut map = Value::from(BTreeMap::default());
    /// map.insert("bar", inner_map.clone());
    ///
    /// assert_eq!(map.get("bar").unwrap(), Some(&Value::from(inner_map)));
    ///
    /// let lookup_key = Lookup::from_str("bar.baz").unwrap();
    /// assert_eq!(map.get(lookup_key).unwrap(), Some(&Value::from(1)));
    /// ```
    #[allow(clippy::missing_errors_doc)]
    pub fn get<'a>(
        &self,
        lookup: impl Into<Lookup<'a>> + Debug,
    ) -> StdResult<Option<&Self>, ValueError> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("get", lookup = %working_lookup);
        let _guard = span.enter();

        let this_segment = working_lookup.pop_front();
        match (this_segment, self) {
            // We've met an end and found our value.
            (None, item) => Ok(Some(item)),
            // Descend into a coalesce
            (Some(Segment::Coalesce(sub_segments)), value) => {
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = Lookup::from(sub_segment);
                    // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                    // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the get.
                    if value.contains(lookup.clone()) {
                        needle = Some(lookup);
                        break;
                    }
                }
                match needle {
                    Some(needle) => value.get(needle),
                    None => Ok(None),
                }
            }
            // Descend into a map
            (Some(Segment::Field(Field { name, .. })), Value::Object(map)) => match map.get(name) {
                Some(inner) => inner.get(working_lookup.clone()),
                None => Ok(None),
            },
            (Some(Segment::Index(_)), Value::Object(_)) => Ok(None),
            // Descend into an array
            (Some(Segment::Index(i)), Value::Array(array)) => {
                let index = if i.is_negative() {
                    if i.abs() > array.len() as isize {
                        // The index is before the start of the array.
                        return Ok(None);
                    }
                    (array.len() as isize + i) as usize
                } else {
                    i as usize
                };

                match array.get(index) {
                    Some(inner) => inner.get(working_lookup.clone()),
                    None => Ok(None),
                }
            }
            (Some(Segment::Field(Field { .. })), Value::Array(_)) => {
                trace!("Mismatched field trying to access array.");
                Ok(None)
            }
            // This is just not allowed!
            (
                Some(_s),
                Value::Boolean(_)
                | Value::Bytes(_)
                | Value::Regex(_)
                | Value::Timestamp(_)
                | Value::Float(_)
                | Value::Integer(_)
                | Value::Null,
            ) => {
                trace!("Mismatched primitive field while trying to use segment.");
                Ok(None)
            }
        }
    }

    /// Returns a reference to a field value specified by a path iter.
    #[allow(clippy::needless_pass_by_value)]
    pub fn get_by_path_v2<'a>(&self, path: impl Path<'a>) -> Option<&Self> {
        let mut value = self;
        let mut path_iter = path.segment_iter();
        loop {
            match (path_iter.next(), value) {
                (None, _) => return Some(value),
                (Some(BorrowedSegment::Field(key)), Value::Object(map)) => {
                    match map.get(key.as_ref()) {
                        None => return None,
                        Some(nested_value) => {
                            value = nested_value;
                        }
                    }
                }
                (Some(BorrowedSegment::Index(index)), Value::Array(array)) => {
                    match array.get(index as usize) {
                        None => return None,
                        Some(nested_value) => {
                            value = nested_value;
                        }
                    }
                }
                _ => return None,
            }
        }
    }

    /// Get a mutable borrow of the value by lookup.
    ///
    /// ```rust
    /// use value::Value;
    /// use lookup::Lookup;
    /// use std::collections::BTreeMap;
    ///
    /// let mut inner_map = Value::from(BTreeMap::default());
    /// inner_map.insert("baz", 1);
    ///
    /// let mut map = Value::from(BTreeMap::default());
    /// map.insert("bar", inner_map.clone());
    ///
    /// assert_eq!(map.get_mut("bar").unwrap(), Some(&mut Value::from(inner_map)));
    ///
    /// let lookup_key = Lookup::from_str("bar.baz").unwrap();
    /// assert_eq!(map.get_mut(lookup_key).unwrap(), Some(&mut Value::from(1)));
    /// ```
    ///
    /// # Panics
    ///
    /// This function may panic if an invariant is violated, indicating a
    /// serious bug.
    #[allow(clippy::missing_errors_doc)]
    pub fn get_mut<'a>(
        &mut self,
        lookup: impl Into<Lookup<'a>> + Debug,
    ) -> StdResult<Option<&mut Self>, ValueError> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("get_mut", lookup = %working_lookup);
        let _guard = span.enter();

        let this_segment = working_lookup.pop_front();
        match (this_segment, self) {
            // We've met an end and found our value.
            (None, item) => Ok(Some(item)),
            // This is just not allowed!
            (
                _,
                Value::Boolean(_)
                | Value::Bytes(_)
                | Value::Regex(_)
                | Value::Timestamp(_)
                | Value::Float(_)
                | Value::Integer(_)
                | Value::Null,
            ) => unimplemented!(),
            // Descend into a coalesce
            (Some(Segment::Coalesce(sub_segments)), value) => {
                // Creating a needle with a back out of the loop is very important.
                let mut needle = None;
                for sub_segment in sub_segments {
                    let mut lookup = Lookup::from(sub_segment);
                    lookup.extend(working_lookup.clone()); // We need to include the rest of the get.
                                                           // Notice we cannot take multiple mutable borrows in a loop, so we must pay the
                                                           // contains cost extra. It's super unfortunate, hopefully future work can solve this.
                    if value.contains(lookup.clone()) {
                        needle = Some(lookup);
                        break;
                    }
                }
                match needle {
                    Some(needle) => value.get_mut(needle),
                    None => Ok(None),
                }
            }
            // Descend into a map
            (Some(Segment::Field(Field { name, .. })), Value::Object(map)) => {
                match map.get_mut(name) {
                    Some(inner) => inner.get_mut(working_lookup.clone()),
                    None => Ok(None),
                }
            }
            (Some(Segment::Index(_)), Value::Object(_))
            | (Some(Segment::Field(_)), Value::Array(_)) => Ok(None),
            // Descend into an array
            (Some(Segment::Index(i)), Value::Array(array)) => {
                let index = if i.is_negative() {
                    if i.abs() > array.len() as isize {
                        // The index is before the start of the array.
                        return Ok(None);
                    }
                    (array.len() as isize + i) as usize
                } else {
                    i as usize
                };

                match array.get_mut(index) {
                    Some(inner) => inner.get_mut(working_lookup.clone()),
                    None => Ok(None),
                }
            }
        }
    }

    /// Get a mutable borrow of the value by path
    #[allow(clippy::needless_pass_by_value)]
    pub fn get_mut_by_path_v2<'a>(&mut self, path: impl Path<'a>) -> Option<&mut Self> {
        let mut value = self;
        let mut path_iter = path.segment_iter();
        loop {
            match (path_iter.next(), value) {
                (None, value) => return Some(value),
                (Some(BorrowedSegment::Field(key)), Value::Object(map)) => {
                    match map.get_mut(key.as_ref()) {
                        None => return None,
                        Some(nested_value) => {
                            value = nested_value;
                        }
                    }
                }
                (Some(BorrowedSegment::Index(index)), Value::Array(array)) => {
                    match array.get_mut(index as usize) {
                        None => return None,
                        Some(nested_value) => {
                            value = nested_value;
                        }
                    }
                }
                _ => return None,
            }
        }
    }

    /// Determine if the lookup is contained within the value.
    ///
    /// ```rust
    /// use value::Value;
    /// use lookup::Lookup;
    /// use std::collections::BTreeMap;
    ///
    /// let mut inner_map = Value::from(BTreeMap::default());
    /// inner_map.insert("baz", 1);
    ///
    /// let mut map = Value::from(BTreeMap::default());
    /// map.insert("bar", inner_map.clone());
    ///
    /// assert!(map.contains("bar"));
    ///
    /// let lookup_key = Lookup::from_str("bar.baz").unwrap();
    /// assert!(map.contains(lookup_key));
    /// ```
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
    ///
    /// ```rust
    /// use value::Value;
    /// use lookup::{Lookup, LookupBuf};
    /// let plain_key = "lick";
    /// let lookup_key = LookupBuf::from_str("vic.stick.slam").unwrap();
    /// let mut value = Value::from(std::collections::BTreeMap::default());
    /// value.insert(plain_key, 1);
    /// value.insert(lookup_key, 2);
    ///
    /// let mut keys = value.lookups(None, false);
    /// assert_eq!(keys.next(), Some(Lookup::root()));
    /// assert_eq!(keys.next(), Some(Lookup::from_str("lick").unwrap()));
    /// assert_eq!(keys.next(), Some(Lookup::from_str("vic").unwrap()));
    /// assert_eq!(keys.next(), Some(Lookup::from_str("vic.stick").unwrap()));
    /// assert_eq!(keys.next(), Some(Lookup::from_str("vic.stick.slam").unwrap()));
    ///
    /// let mut keys = value.lookups(None, true);
    /// assert_eq!(keys.next(), Some(Lookup::from_str("lick").unwrap()));
    /// assert_eq!(keys.next(), Some(Lookup::from_str("vic.stick.slam").unwrap()));
    /// ```
    #[instrument(level = "trace", skip(self, prefix, only_leaves))]
    pub fn lookups<'a>(
        &'a self,
        prefix: Option<Lookup<'a>>,
        only_leaves: bool,
    ) -> Box<dyn Iterator<Item = Lookup<'a>> + 'a> {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Regex(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => Box::new(prefix.into_iter()),
            Value::Object(m) => {
                let this = prefix
                    .clone()
                    .or_else(|| Some(Lookup::default()))
                    .into_iter();
                let children = m.iter().flat_map(move |(k, v)| {
                    let lookup = prefix.clone().map_or_else(
                        || Lookup::from(k),
                        |mut l| {
                            l.push_back(Segment::from(k.as_str()));
                            l
                        },
                    );
                    v.lookups(Some(lookup), only_leaves)
                });

                if only_leaves && !self.is_empty() {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
            Value::Array(a) => {
                let this = prefix
                    .clone()
                    .or_else(|| Some(Lookup::default()))
                    .into_iter();
                let children = a.iter().enumerate().flat_map(move |(k, v)| {
                    let lookup = prefix.clone().map_or_else(
                        || Lookup::from(k as isize),
                        |mut l| {
                            l.push_back(Segment::index(k as isize));
                            l
                        },
                    );
                    v.lookups(Some(lookup), only_leaves)
                });

                if only_leaves && !self.is_empty() {
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
    ///
    /// ```rust
    /// use value::Value;
    /// use lookup::{Lookup, LookupBuf};
    /// let plain_key = "lick";
    /// let lookup_key = LookupBuf::from_str("vic.stick.slam").unwrap();
    /// let mut value = Value::from(std::collections::BTreeMap::default());
    /// value.insert(plain_key, 1);
    /// value.insert(lookup_key, 2);
    ///
    /// let mut keys = value.pairs(None, false);
    /// assert_eq!(keys.next(), Some((Lookup::root(), &Value::from({
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
    /// let mut keys = value.pairs(None, true);
    /// assert_eq!(keys.next(), Some((Lookup::from_str("lick").unwrap(), &Value::from(1))));
    /// assert_eq!(keys.next(), Some((Lookup::from_str("vic.stick.slam").unwrap(), &Value::from(2))));
    /// ```
    #[instrument(level = "trace", skip(self, prefix, only_leaves))]
    pub fn pairs<'a>(
        &'a self,
        prefix: Option<Lookup<'a>>,
        only_leaves: bool,
    ) -> Box<dyn Iterator<Item = (Lookup<'a>, &'a Self)> + 'a> {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Regex(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => Box::new(prefix.map(move |v| (v, self)).into_iter()),
            Value::Object(m) => {
                let this = prefix
                    .clone()
                    .or_else(|| Some(Lookup::default()))
                    .map(|v| (v, self))
                    .into_iter();
                let children = m.iter().flat_map(move |(k, v)| {
                    let lookup = prefix.clone().map_or_else(
                        || Lookup::from(k),
                        |mut l| {
                            l.push_back(Segment::from(k.as_str()));
                            l
                        },
                    );
                    v.pairs(Some(lookup), only_leaves)
                });

                if only_leaves && !self.is_empty() {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
            Value::Array(a) => {
                let this = prefix
                    .clone()
                    .or_else(|| Some(Lookup::default()))
                    .map(|v| (v, self))
                    .into_iter();
                let children = a.iter().enumerate().flat_map(move |(k, v)| {
                    let lookup = prefix.clone().map_or_else(
                        || Lookup::from(k as isize),
                        |mut l| {
                            l.push_back(Segment::index(k as isize));
                            l
                        },
                    );
                    v.pairs(Some(lookup), only_leaves)
                });

                if only_leaves && !self.is_empty() {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
        }
    }
}

/// Converts a timestamp to a `String`.
#[must_use]
pub fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

#[cfg(test)]
mod test {
    use quickcheck::{QuickCheck, TestResult};

    use super::*;

    mod value_compare {
        use super::*;

        #[test]
        fn compare_correctly() {
            assert!(Value::Integer(0).eq(&Value::Integer(0)));
            assert!(!Value::Integer(0).eq(&Value::Integer(1)));
            assert!(!Value::Boolean(true).eq(&Value::Integer(2)));
            assert!(Value::from(1.2).eq(&Value::from(1.4)));
            assert!(!Value::from(1.2).eq(&Value::from(-1.2)));
            assert!(!Value::from(-0.0).eq(&Value::from(0.0)));
            assert!(!Value::from(f64::NEG_INFINITY).eq(&Value::from(f64::INFINITY)));
            assert!(Value::Array(vec![Value::Integer(0), Value::Boolean(true)])
                .eq(&Value::Array(vec![Value::Integer(0), Value::Boolean(true)])));
            assert!(!Value::Array(vec![Value::Integer(0), Value::Boolean(true)])
                .eq(&Value::Array(vec![Value::Integer(1), Value::Boolean(true)])));
        }
    }

    mod value_hash {
        use super::*;

        fn hash(a: &Value) -> u64 {
            let mut h = std::collections::hash_map::DefaultHasher::new();

            a.hash(&mut h);
            h.finish()
        }

        #[test]
        fn hash_correctly() {
            assert_eq!(hash(&Value::Integer(0)), hash(&Value::Integer(0)));
            assert_ne!(hash(&Value::Integer(0)), hash(&Value::Integer(1)));
            assert_ne!(hash(&Value::Boolean(true)), hash(&Value::Integer(2)));
            assert_eq!(hash(&Value::from(1.2)), hash(&Value::from(1.4)));
            assert_ne!(hash(&Value::from(1.2)), hash(&Value::from(-1.2)));
            assert_ne!(hash(&Value::from(-0.0)), hash(&Value::from(0.0)));
            assert_ne!(
                hash(&Value::from(f64::NEG_INFINITY)),
                hash(&Value::from(f64::INFINITY))
            );
            assert_eq!(
                hash(&Value::Array(vec![Value::Integer(0), Value::Boolean(true)])),
                hash(&Value::Array(vec![Value::Integer(0), Value::Boolean(true)]))
            );
            assert_ne!(
                hash(&Value::Array(vec![Value::Integer(0), Value::Boolean(true)])),
                hash(&Value::Array(vec![Value::Integer(1), Value::Boolean(true)]))
            );
        }
    }

    mod insert_get_remove {
        use super::*;

        #[test]
        fn single_field() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_object().unwrap()[key], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
        }

        #[test]
        fn nested_field() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root.doot";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(
                value.as_object().unwrap()["root"].as_object().unwrap()["doot"],
                marker
            );
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
        }

        #[test]
        fn double_nested_field() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root.doot.toot";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(
                value.as_object().unwrap()["root"].as_object().unwrap()["doot"]
                    .as_object()
                    .unwrap()["toot"],
                marker
            );
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
        }

        #[test]
        fn single_index() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_array_unwrap()[0], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
        }

        #[test]
        fn negative_index() {
            let mut value = Value::from(vec![Value::from(1), Value::from(2), Value::from(3)]);
            let key = "[-2]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let marker = Value::from(true);

            assert_eq!(
                value.insert(lookup.clone(), marker.clone()).unwrap(),
                Some(Value::from(2))
            );
            assert_eq!(value.as_array_unwrap().len(), 3);
            assert_eq!(value.as_array_unwrap()[0], Value::from(1));
            assert_eq!(value.as_array_unwrap()[1], marker);
            assert_eq!(value.as_array_unwrap()[2], Value::from(3));
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));

            let lookup = Lookup::from_str(key).unwrap();
            assert_eq!(value.remove(lookup, true).unwrap(), Some(marker));
            assert_eq!(value.as_array_unwrap().len(), 2);
            assert_eq!(value.as_array_unwrap()[0], Value::from(1));
            assert_eq!(value.as_array_unwrap()[1], Value::from(3));
        }

        #[test]
        fn negative_index_resize() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[-3]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let marker = Value::from(true);

            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_array_unwrap().len(), 3);
            assert_eq!(value.as_array_unwrap()[0], marker);
            assert_eq!(value.as_array_unwrap()[1], Value::Null);
            assert_eq!(value.as_array_unwrap()[2], Value::Null);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
        }

        #[test]
        fn nested_index() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0][0]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(value.as_array_unwrap()[0].as_array_unwrap()[0], marker);
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
        }

        #[test]
        fn field_index() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root[0]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(
                value.as_object().unwrap()["root"].as_array_unwrap()[0],
                marker
            );
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
        }

        #[test]
        fn field_negative_index() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root[-1]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let marker = Value::from(true);

            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None,);
            assert_eq!(
                value.as_object().unwrap()["root"].as_array_unwrap()[0],
                marker
            );
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.remove(&lookup, true).unwrap(), Some(marker),);
            assert_eq!(value, Value::from(BTreeMap::default()),);
        }

        #[test]
        fn index_field() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0].boot";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(
                value.as_array_unwrap()[0].as_object().unwrap()["boot"],
                marker
            );
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
        }

        #[test]
        fn nested_index_field() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0][0].boot";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(
                value.as_array_unwrap()[0].as_array_unwrap()[0]
                    .as_object()
                    .unwrap()["boot"],
                marker
            );
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
        }

        #[test]
        fn nested_index_negative() {
            let mut value = Value::from(BTreeMap::default());
            let key = "field[0][-1]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(
                value.as_object().unwrap()["field"].as_array_unwrap()[0].as_array_unwrap()[0],
                marker
            );
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker),);
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker),);
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker),);
        }

        #[test]
        fn field_with_nested_index_field() {
            let mut value = Value::from(BTreeMap::default());
            let key = "root[0][0].boot";
            let lookup = LookupBuf::from_str(key).unwrap();
            let mut marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            assert_eq!(
                value.as_object().unwrap()["root"].as_array_unwrap()[0].as_array_unwrap()[0]
                    .as_object()
                    .unwrap()["boot"],
                marker
            );
            assert_eq!(value.get(&lookup).unwrap(), Some(&marker));
            assert_eq!(value.get_mut(&lookup).unwrap(), Some(&mut marker));
            assert_eq!(value.remove(&lookup, false).unwrap(), Some(marker));
        }

        #[test]
        fn populated_field() {
            let mut value = Value::from(BTreeMap::default());
            let marker = Value::from(true);
            let lookup = LookupBuf::from_str("a[2]").unwrap();
            assert_eq!(value.insert(lookup, marker.clone()).unwrap(), None);

            let lookup = LookupBuf::from_str("a[0]").unwrap();
            assert_eq!(
                value.insert(lookup, marker.clone()).unwrap(),
                Some(Value::Null)
            );

            assert_eq!(value.as_object().unwrap()["a"].as_array_unwrap().len(), 3);
            assert_eq!(value.as_object().unwrap()["a"].as_array_unwrap()[0], marker);
            assert_eq!(
                value.as_object().unwrap()["a"].as_array_unwrap()[1],
                Value::Null
            );
            assert_eq!(value.as_object().unwrap()["a"].as_array_unwrap()[2], marker);

            // Replace the value at 0.
            let lookup = LookupBuf::from_str("a[0]").unwrap();
            let marker = Value::from(false);
            assert_eq!(
                value.insert(lookup, marker.clone()).unwrap(),
                Some(Value::from(true))
            );
            assert_eq!(value.as_object().unwrap()["a"].as_array_unwrap()[0], marker);
        }
    }

    mod corner_cases {
        use super::*;

        #[test]
        fn remove_prune_map_with_map() {
            let mut value = Value::from(BTreeMap::default());
            let key = "foo.bar";
            let lookup = LookupBuf::from_str(key).unwrap();
            let marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            // Since the `foo` map is now empty, this should get cleaned.
            assert_eq!(value.remove(&lookup, true).unwrap(), Some(marker));
            assert!(!value.contains("foo"));
        }

        #[test]
        fn remove_prune_map_with_array() {
            let mut value = Value::from(BTreeMap::default());
            let key = "foo[0]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            // Since the `foo` map is now empty, this should get cleaned.
            assert_eq!(value.remove(&lookup, true).unwrap(), Some(marker));
            assert!(!value.contains("foo"));
        }

        #[test]
        fn remove_prune_array_with_map() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0].bar";
            let lookup = LookupBuf::from_str(key).unwrap();
            let marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            // Since the `foo` map is now empty, this should get cleaned.
            assert_eq!(value.remove(&lookup, true).unwrap(), Some(marker));
            assert!(!value.contains(0));
        }

        #[test]
        fn remove_prune_array_with_array() {
            let mut value = Value::from(Vec::<Value>::default());
            let key = "[0][0]";
            let lookup = LookupBuf::from_str(key).unwrap();
            let marker = Value::from(true);
            assert_eq!(value.insert(lookup.clone(), marker.clone()).unwrap(), None);
            // Since the `foo` map is now empty, this should get cleaned.
            assert_eq!(value.remove(&lookup, true).unwrap(), Some(marker));
            assert!(!value.contains(0));
        }
    }

    #[test]
    fn quickcheck_value() {
        fn inner(mut path: LookupBuf) -> TestResult {
            let mut value = Value::from(BTreeMap::default());
            let mut marker = Value::from(true);

            if matches!(path.get(0), Some(SegmentBuf::Index(_))) {
                // Push a field at the start of the path since the top level is always a map.
                path.push_front(SegmentBuf::from("field"));
            }

            assert_eq!(
                value.insert(path.clone(), marker.clone()).unwrap(),
                None,
                "inserting value"
            );
            assert_eq!(value.get(&path).unwrap(), Some(&marker), "retrieving value");
            assert_eq!(
                value.get_mut(&path).unwrap(),
                Some(&mut marker),
                "retrieving mutable value"
            );

            assert_eq!(
                value.remove(&path, true).unwrap(),
                Some(marker),
                "removing value"
            );

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .max_tests(200)
            .quickcheck(inner as fn(LookupBuf) -> TestResult);
    }
}
