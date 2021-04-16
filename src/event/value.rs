use crate::{event::error::EventError, event::timestamp_to_string, Result};
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use derive_is_enum_variant::is_enum_variant;
use lookup::{Field, FieldBuf, Lookup, LookupBuf, Segment, SegmentBuf};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::{BTreeMap, VecDeque};
use std::convert::{TryFrom, TryInto};
use std::fmt::Debug;
use std::iter::FromIterator;
use toml::value::Value as TomlValue;

#[derive(PartialEq, Debug, Clone, Deserialize, is_enum_variant)]
pub enum Value {
    Bytes(Bytes),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
    Map(BTreeMap<String, Value>),
    Array(Vec<Value>),
    Null,
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

impl From<vrl::Value> for Value {
    fn from(v: vrl::Value) -> Self {
        use vrl::Value::*;

        match v {
            Bytes(v) => Value::Bytes(v),
            Integer(v) => Value::Integer(v),
            Float(v) => Value::Float(*v),
            Boolean(v) => Value::Boolean(v),
            Object(v) => Value::Map(v.into_iter().map(|(k, v)| (k, v.into())).collect()),
            Array(v) => Value::Array(v.into_iter().map(Into::into).collect()),
            Timestamp(v) => Value::Timestamp(v),
            Regex(v) => Value::Bytes(bytes::Bytes::copy_from_slice(v.to_string().as_bytes())),
            Null => Value::Null,
        }
    }
}

impl From<Value> for vrl::Value {
    fn from(v: Value) -> Self {
        use vrl::Value::*;

        match v {
            Value::Bytes(v) => v.into(),
            Value::Integer(v) => v.into(),
            Value::Float(v) => v.into(),
            Value::Boolean(v) => v.into(),
            Value::Map(v) => Object(v.into_iter().map(|(k, v)| (k, v.into())).collect()),
            Value::Array(v) => Array(v.into_iter().map(Into::into).collect()),
            Value::Timestamp(v) => v.into(),
            Value::Null => ().into(),
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

    pub fn as_bytes(&self) -> Bytes {
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

    pub fn into_bytes(self) -> Bytes {
        self.as_bytes()
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Value>> {
        match &self {
            Value::Map(map) => Some(map),
            _ => None,
        }
    }

    pub fn as_timestamp(&self) -> Option<&DateTime<Utc>> {
        match &self {
            Value::Timestamp(ts) => Some(ts),
            _ => None,
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

    /// Merges `incoming` value into self.
    ///
    /// Will concatenate `Bytes` and overwrite the rest value kinds.
    pub fn merge(&mut self, incoming: Value) {
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

    /// Lookup API methods
    /// TODO Add the tests

    /// Return if the node is a leaf (meaning it has no children) or not.
    ///
    /// This is notably useful for things like influxdb logs where we list only leaves.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
    /// use std::collections::BTreeMap;
    ///
    /// let val = Value::from(1);
    /// assert_eq!(val.is_leaf(), true);
    ///
    /// let mut val = Value::from(Vec::<Value>::default());
    /// assert_eq!(val.is_leaf(), true);
    /// val.insert(0, 1);
    /// assert_eq!(val.is_leaf(), false);
    /// val.insert(3, 1);
    /// assert_eq!(val.is_leaf(), false);
    ///
    /// let mut val = Value::from(BTreeMap::default());
    /// assert_eq!(val.is_leaf(), true);
    /// val.insert("foo", 1);
    /// assert_eq!(val.is_leaf(), false);
    /// val.insert("bar", 2);
    /// assert_eq!(val.is_leaf(), false);
    /// ```
    #[instrument(level = "trace")]
    pub fn is_leaf<'a>(&'a self) -> bool {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => true,
            Value::Map(_) => self.is_empty(),
            Value::Array(_) => self.is_empty(),
        }
    }

    /// Return if the node is empty, that is, it is an array or map with no items.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
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
    #[instrument(level = "trace")]
    pub fn is_empty(&self) -> bool {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_) => false,
            Value::Null => true,
            Value::Map(v) => v.is_empty(),
            Value::Array(v) => v.is_empty(),
        }
    }

    /// Return the number of subvalues the value has.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
    /// use std::collections::BTreeMap;
    ///
    /// let val = Value::from(1);
    /// assert_eq!(val.len(), None);
    ///
    /// let mut val = Value::from(Vec::<Value>::default());
    /// assert_eq!(val.len(), Some(0));
    /// val.insert(0, 1);
    /// assert_eq!(val.len(), Some(1));
    /// val.insert(3, 1);
    /// assert_eq!(val.len(), Some(4));
    ///
    /// let mut val = Value::from(BTreeMap::default());
    /// assert_eq!(val.len(), Some(0));
    /// val.insert("foo", 1);
    /// assert_eq!(val.len(), Some(1));
    /// val.insert("bar", 2);
    /// assert_eq!(val.len(), Some(2));
    /// ```
    #[instrument(level = "trace")]
    pub fn len(&self) -> Option<usize> {
        match &self {
            Value::Boolean(_)
            | Value::Bytes(_)
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => None,
            Value::Map(v) => Some(v.len()),
            Value::Array(v) => Some(v.len()),
        }
    }

    fn insert_coalesce(
        sub_segments: Vec<FieldBuf>,
        working_lookup: LookupBuf,
        sub_value: &mut Value,
        value: Value,
    ) -> std::result::Result<Option<Value>, EventError> {
        // Creating a needle with a back out of the loop is very important.
        let mut needle = None;
        for sub_segment in sub_segments {
            let mut lookup = LookupBuf::from(sub_segment);
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
            Some(needle) => sub_value.insert(needle, value),
            None => Ok(None),
        }
    }

    fn insert_map(
        name: &str,
        requires_quoting: bool,
        mut working_lookup: LookupBuf,
        map: &mut BTreeMap<String, Value>,
        value: Value,
    ) -> std::result::Result<Option<Value>, EventError> {
        trace!(field = %name, "Seeking into map.");
        let next_value = match working_lookup.get(0) {
            Some(SegmentBuf::Index(next_len)) => {
                Value::Array(Vec::with_capacity(*next_len as usize))
            }
            Some(SegmentBuf::Field(FieldBuf { .. })) => Value::Map(Default::default()),
            Some(SegmentBuf::Coalesce(set)) => {
                Value::Map(Default::default())
                /*
                let mut cursor_set = set;
                loop {
                    match cursor_set.get(0).and_then(|v| v.get(0)) {
                        None => return Err(EventError::EmptyCoalesceSubSegment),
                        Some(SegmentBuf::Field(FieldBuf { .. })) => {
                            break {
                                trace!("Preparing inner map.");
                                Value::Map(Default::default())
                            }
                        }
                        Some(SegmentBuf::Index(_)) => {
                            break {
                                trace!("Preparing inner array.");
                                Value::Array(Vec::with_capacity(0))
                            }
                        }
                        Some(SegmentBuf::Coalesce(set)) => cursor_set = &set,
                    }
                }
                */
            }
            None => {
                trace!(field = %name, "Inserted.");
                return Ok(map.insert(name.to_string(), value));
            }
        };
        map.entry(name.to_string())
            .or_insert_with(|| {
                trace!(key = ?name, "Pushing required next value onto map.");
                next_value
            })
            .insert(working_lookup, value)
            .map_err(|mut e| {
                if let EventError::PrimitiveDescent {
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

    /// Note this will blow up with negative indexes which it wasn't designed to cope with,
    /// but since Remap handles it, we should handle it here too..
    fn insert_array(
        i: isize,
        mut working_lookup: LookupBuf,
        array: &mut Vec<Value>,
        value: Value,
    ) -> std::result::Result<Option<Value>, EventError> {
        {
            match array.get_mut(i as usize) {
                Some(inner) => {
                    trace!(index = ?i, "Seeking into array.");
                    inner.insert(working_lookup, value).map_err(|mut e| {
                        if let EventError::PrimitiveDescent {
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
                }
                None => {
                    trace!(lenth = ?i, "Array too small, resizing array to fit.");
                    // Fill the vector to the index.
                    array.resize(i as usize, Value::Null);
                    let mut retval = Ok(None);
                    let next_val = match working_lookup.get(0) {
                        Some(SegmentBuf::Index(next_len)) => {
                            let mut inner = Value::Array(Vec::with_capacity(*next_len as usize));
                            trace!("Preparing inner array.");
                            retval = inner.insert(working_lookup, value).map_err(|mut e| {
                                if let EventError::PrimitiveDescent {
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
                            let mut inner = Value::Map(Default::default());
                            let name = name.clone(); // This is for navigating an ownership issue in the error stack reporting.
                            let requires_quoting = *requires_quoting; // This is for navigating an ownership issue in the error stack reporting.
                            trace!("Preparing inner map.");
                            retval = inner.insert(working_lookup, value).map_err(|mut e| {
                                if let EventError::PrimitiveDescent {
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
                        Some(SegmentBuf::Coalesce(set)) => {
                            match set.get(0) {
                                None => return Err(EventError::EmptyCoalesceSubSegment),
                                Some(_) => {
                                    let mut inner = Value::Map(Default::default());
                                    trace!("Preparing inner map.");
                                    let set = SegmentBuf::Coalesce(set.clone());
                                    retval =
                                        inner.insert(working_lookup, value).map_err(|mut e| {
                                            if let EventError::PrimitiveDescent {
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
                            }
                            /*
                            let mut cursor_set = set;
                            loop {
                                match cursor_set.get(0).and_then(|v| v.get(0)) {
                                    None => return Err(EventError::EmptyCoalesceSubSegment),
                                    Some(SegmentBuf::Field(FieldBuf { .. })) => {
                                        break {
                                            let mut inner = Value::Map(Default::default());
                                            trace!("Preparing inner map.");
                                            let set = SegmentBuf::Coalesce(set.clone());
                                            retval = inner.insert(working_lookup, value).map_err(
                                                |mut e| {
                                                    if let EventError::PrimitiveDescent {
                                                        original_target,
                                                        primitive_at,
                                                        original_value: _,
                                                    } = &mut e
                                                    {
                                                        original_target.push_front(set.clone());
                                                        primitive_at.push_front(set.clone());
                                                    };
                                                    e
                                                },
                                            );
                                            inner
                                        }
                                    }
                                    Some(SegmentBuf::Index(i)) => {
                                        break {
                                            let mut inner = Value::Array(Vec::with_capacity(0));
                                            trace!("Preparing inner array.");
                                            let segment = SegmentBuf::Index(*i); // This is for navigating an ownership issue in the error stack reporting.
                                            retval = inner.insert(working_lookup, value).map_err(
                                                |mut e| {
                                                    if let EventError::PrimitiveDescent {
                                                        original_target,
                                                        primitive_at,
                                                        original_value: _,
                                                    } = &mut e
                                                    {
                                                        original_target.push_front(segment.clone());
                                                        primitive_at.push_front(segment.clone());
                                                    };
                                                    e
                                                },
                                            );
                                            inner
                                        };
                                    }
                                    Some(SegmentBuf::Coalesce(set)) => cursor_set = set,
                                }
                            }
                             */
                        }
                        None => value,
                    };
                    trace!(?next_val, "Setting index to value.");
                    array.push(next_val);
                    retval
                }
            }
        }
    }

    /// This situation is surprisingly common due to how nulls full sparse vectors.
    fn insert_null(
        segment: &SegmentBuf,
        mut working_lookup: LookupBuf,
        val: &mut Value,
        value: Value,
    ) -> std::result::Result<Option<Value>, EventError> {
        let retval;
        let this_val = match segment {
            SegmentBuf::Index(_) => {
                let mut inner = Value::Array(Vec::with_capacity(0));
                trace!("Preparing inner array.");
                working_lookup.push_front(segment.clone());
                retval = inner.insert(working_lookup, value).map_err(|mut e| {
                    if let EventError::PrimitiveDescent {
                        original_target,
                        primitive_at,
                        original_value: _,
                    } = &mut e
                    {
                        original_target.push_front(segment.clone());
                        primitive_at.push_front(segment.clone());
                    };
                    e
                });
                inner
            }
            SegmentBuf::Field(FieldBuf { .. }) => {
                let mut inner = Value::Map(Default::default());
                trace!("Preparing inner map.");
                working_lookup.push_front(segment.clone());
                retval = inner.insert(working_lookup, value).map_err(|mut e| {
                    if let EventError::PrimitiveDescent {
                        original_target,
                        primitive_at,
                        original_value: _,
                    } = &mut e
                    {
                        original_target.push_front(segment.clone());
                        primitive_at.push_front(segment.clone());
                    };
                    e
                });
                inner
            }
            SegmentBuf::Coalesce(set) => {
                todo!()
                /*
                let mut cursor_set = &set;
                loop {
                    match cursor_set.get(0).and_then(|v| v.get(0)) {
                        None => return Err(EventError::EmptyCoalesceSubSegment),
                        Some(SegmentBuf::Field(FieldBuf { .. })) => {
                            break {
                                let mut inner = Value::Map(Default::default());
                                trace!("Preparing inner map.");
                                let set = SegmentBuf::Coalesce(set.clone());
                                retval = inner.insert(working_lookup, value).map_err(|mut e| {
                                    if let EventError::PrimitiveDescent {
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
                        }
                        Some(SegmentBuf::Index(_)) => {
                            break {
                                let mut inner = Value::Array(Vec::with_capacity(0));
                                trace!("Preparing inner array.");
                                let set = SegmentBuf::Coalesce(set.clone());
                                retval = inner.insert(working_lookup, value).map_err(|mut e| {
                                    if let EventError::PrimitiveDescent {
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
                        }
                        Some(SegmentBuf::Coalesce(set)) => cursor_set = &set,
                    }
                }
                */
            }
        };
        trace!(val = ?this_val, "Setting previously existing null to value.");
        *val = this_val;
        retval
    }

    /// Insert a value at a given lookup.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
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
    pub fn insert(
        &mut self,
        lookup: impl Into<LookupBuf> + Debug,
        value: impl Into<Value> + Debug,
    ) -> std::result::Result<Option<Value>, EventError> {
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
                trace!("Swapped with existing value.");
                Ok(Some(value))
            }
            // This is just not allowed!
            (Some(segment), Value::Boolean(_))
            | (Some(segment), Value::Bytes(_))
            | (Some(segment), Value::Timestamp(_))
            | (Some(segment), Value::Float(_))
            | (Some(segment), Value::Integer(_)) => {
                trace!("Encountered descent into a primitive.");
                Err(EventError::PrimitiveDescent {
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
                Value::insert_coalesce(sub_segments, working_lookup, sub_value, value)
            }
            // Descend into a map
            (
                Some(SegmentBuf::Field(FieldBuf {
                    ref name,
                    ref requires_quoting,
                })),
                Value::Map(ref mut map),
            ) => Value::insert_map(name, *requires_quoting, working_lookup, map, value),
            (Some(SegmentBuf::Index(_)), Value::Map(_)) => {
                trace!("Mismatched index trying to access map.");
                Ok(None)
            }
            // Descend into an array
            (Some(SegmentBuf::Index(i)), Value::Array(ref mut array)) => {
                Value::insert_array(i, working_lookup, array, value)
            }
            (Some(SegmentBuf::Field(FieldBuf { .. })), Value::Array(_)) => {
                trace!("Mismatched field trying to access array.");
                Ok(None)
            }
            (Some(segment), val) if val == &mut Value::Null => {
                Value::insert_null(&segment, working_lookup, val, value)
            }
            (Some(_), Value::Null) => unreachable!("This is covered by the above case."),
        }
    }

    /// Remove a value that exists at a given lookup.
    ///
    /// Setting `prune` to true will also remove the entries of maps and arrays that are emptied.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
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
    pub fn remove<'a>(
        &mut self,
        lookup: impl Into<Lookup<'a>> + Debug,
        prune: bool,
    ) -> std::result::Result<Option<Value>, EventError> {
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
            (Some(segment), Value::Boolean(_))
            | (Some(segment), Value::Bytes(_))
            | (Some(segment), Value::Timestamp(_))
            | (Some(segment), Value::Float(_))
            | (Some(segment), Value::Integer(_))
            | (Some(segment), Value::Null) => {
                if working_lookup.len() > 0 {
                    trace!("Encountered descent into a primitive.");
                    Err(EventError::PrimitiveDescent {
                        primitive_at: LookupBuf::default(),
                        original_target: {
                            let mut l = LookupBuf::from(segment.clone().into_buf());
                            l.extend(working_lookup.into_buf());
                            l
                        },
                        original_value: None,
                    })
                } else {
                    trace!("Cannot remove self. Caller must remove.");
                    Err(EventError::RemovingSelf)
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
                        trace!(option = %lookup, "Found coalesce option.");
                        needle = Some(lookup);
                        break;
                    } else {
                        trace!(option = %lookup, "Did not find coalesce option.");
                    }
                }
                match needle {
                    Some(needle) => value.remove(needle, prune),
                    None => Ok(None),
                }
            }
            // Descend into a map
            (Some(Segment::Field(Field { ref name, .. })), Value::Map(map)) => {
                if working_lookup.len() == 0 {
                    trace!(field = ?name, "Removing from map.");
                    let retval = Ok(map.remove(*name));
                    if map.is_empty() {
                        trace!(prune, "Map is empty. May need to prune.");
                    } else {
                        trace!(
                            prune,
                            items = map.len(),
                            "Map is not empty, no possible pruning."
                        );
                    };
                    retval
                } else {
                    trace!(field = ?name, "Descending into map.");
                    let mut inner_is_empty = false;
                    let retval = match map.get_mut(*name) {
                        Some(inner) => {
                            let ret = inner.remove(working_lookup.clone(), prune);
                            if inner.is_empty() {
                                trace!(prune, "Map is empty. May need to prune.");
                                inner_is_empty = true;
                            } else {
                                trace!(
                                    prune,
                                    items = ?inner.len(),
                                    "Map is not empty, no possible pruning."
                                );
                            };
                            ret
                        }
                        None => Ok(None),
                    };
                    if inner_is_empty && prune {
                        trace!(field = %name, "Pruning.");
                        map.remove(*name);
                    } else {
                        trace!("Pruning not required.");
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
                    let retval = if array.len() > i as usize {
                        Ok(Some(array.remove(i as usize)))
                    } else {
                        Ok(None)
                    };
                    if array.is_empty() {
                        trace!(prune, "Array is empty. May need to prune.");
                    } else {
                        trace!(
                            prune,
                            items = array.len(),
                            "Array is not empty, no possible pruning."
                        );
                    };
                    retval
                } else {
                    trace!(index = ?i, "Descending into array.");
                    let mut inner_is_empty = false;
                    let retval = match array.get_mut(i as usize) {
                        Some(inner) => {
                            let ret = inner.remove(working_lookup.clone(), prune);
                            if inner.is_empty() {
                                trace!(prune, "Inner Array is empty. May need to prune.");
                                inner_is_empty = true
                            } else {
                                trace!(prune, "Inner Array is not empty, no possible pruning.");
                            };
                            ret
                        }
                        None => Ok(None),
                    };
                    if inner_is_empty && prune {
                        trace!("Pruning.");
                        array.remove(i as usize);
                    } else {
                        trace!("Pruning not required.");
                    }
                    retval
                }
            }
            (Some(Segment::Field { .. }), Value::Array(_)) => Ok(None),
        };

        retval
    }

    /// Get an immutable borrow of the value by lookup.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
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
    pub fn get<'a>(
        &self,
        lookup: impl Into<Lookup<'a>> + Debug,
    ) -> std::result::Result<Option<&Value>, EventError> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("get", lookup = %working_lookup);
        let _guard = span.enter();

        let this_segment = working_lookup.pop_front();
        match (this_segment, self) {
            // We've met an end and found our value.
            (None, item) => {
                trace!(?item, "Found.");
                Ok(Some(item))
            }
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
                    }
                    None => Ok(None),
                }
            }
            // Descend into a map
            (Some(Segment::Field(Field { ref name, .. })), Value::Map(map)) => {
                trace!(field = %name, "Descending into map.");
                match map.get(*name) {
                    Some(inner) => inner.get(working_lookup.clone()),
                    None => {
                        trace!("Found nothing.");
                        Ok(None)
                    }
                }
            }
            (Some(Segment::Index(_)), Value::Map(_)) => {
                trace!("Mismatched index trying to access map.");
                Ok(None)
            }
            // Descend into an array
            (Some(Segment::Index(i)), Value::Array(array)) => {
                trace!(index = %i, "Descending into array.");
                match array.get(i as usize) {
                    Some(inner) => inner.get(working_lookup.clone()),
                    None => {
                        trace!("Found nothing.");
                        Ok(None)
                    }
                }
            }
            (Some(Segment::Field(Field { .. })), Value::Array(_)) => {
                trace!("Mismatched field trying to access array.");
                Ok(None)
            }
            // This is just not allowed!
            (Some(_s), Value::Boolean(_))
            | (Some(_s), Value::Bytes(_))
            | (Some(_s), Value::Timestamp(_))
            | (Some(_s), Value::Float(_))
            | (Some(_s), Value::Integer(_))
            | (Some(_s), Value::Null) => {
                trace!("Mismatched primitive field while trying to use segment.");
                Ok(None)
            }
        }
    }

    /// Get a mutable borrow of the value by lookup.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
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
    pub fn get_mut<'a>(
        &mut self,
        lookup: impl Into<Lookup<'a>> + Debug,
    ) -> std::result::Result<Option<&mut Value>, EventError> {
        let mut working_lookup = lookup.into();
        let span = trace_span!("get_mut", lookup = %working_lookup);
        let _guard = span.enter();

        let this_segment = working_lookup.pop_front();
        match (this_segment, self) {
            // We've met an end and found our value.
            (None, item) => {
                trace!(?item, "Found.");
                Ok(Some(item))
            }
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
                    let mut lookup = Lookup::from(sub_segment);
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
                    }
                    None => Ok(None),
                }
            }
            // Descend into a map
            (Some(Segment::Field(Field { ref name, .. })), Value::Map(map)) => {
                match map.get_mut(*name) {
                    Some(inner) => {
                        trace!(field = %name, "Seeking into map.");
                        inner.get_mut(working_lookup.clone())
                    }
                    None => {
                        trace!(field = %name, "Discovered no value to see into.");
                        Ok(None)
                    }
                }
            }
            (Some(Segment::Index(_)), Value::Map(_)) => {
                trace!("Mismatched index trying to access map.");
                Ok(None)
            }
            // Descend into an array
            (Some(Segment::Index(i)), Value::Array(array)) => {
                trace!(index = %i, "Seeking into array.");
                match array.get_mut(i as usize) {
                    Some(inner) => inner.get_mut(working_lookup.clone()),
                    None => Ok(None),
                }
            }
            (Some(Segment::Field(_)), Value::Array(_)) => {
                trace!("Mismatched field trying to access array.");
                Ok(None)
            }
        }
    }

    /// Determine if the lookup is contained within the value.
    ///
    /// ```rust
    /// use shared::{event::*, lookup::*};
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
    /// use shared::{event::*, lookup::*};
    /// let plain_key = "lick";
    /// let lookup_key = LookupBuf::from_str("vic.stick.slam").unwrap();
    /// let mut value = Value::from(std::collections::BTreeMap::default());
    /// value.insert(plain_key, 1);
    /// value.insert(lookup_key, 2);
    ///
    /// let mut keys = value.lookups(None, false);
    /// assert_eq!(keys.next(), Some(Lookup::from_str(".").unwrap()));
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
            | Value::Timestamp(_)
            | Value::Float(_)
            | Value::Integer(_)
            | Value::Null => Box::new(prefix.into_iter().inspect(|v| {
                trace!(prefix = ?v, "Enqueuing leaf for iteration.");
            })),
            Value::Map(m) => {
                trace!(prefix = ?prefix, "Enqueuing for iteration, may have children.");
                let this = prefix
                    .clone()
                    .or_else(|| Some(Lookup::default()))
                    .into_iter();
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

                if only_leaves && !self.is_empty() {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
            Value::Array(a) => {
                trace!(prefix = ?prefix, "Enqueuing for iteration, may have children.");
                let this = prefix
                    .clone()
                    .or_else(|| Some(Lookup::default()))
                    .into_iter();
                let children = a
                    .iter()
                    .enumerate()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k as isize),
                            |mut l| {
                                l.push_back(Segment::index(k as isize));
                                l
                            },
                        );
                        trace!(lookup = ?lookup, "Seeking lookups inside non-leaf element.");
                        v.lookups(Some(lookup), only_leaves)
                    })
                    .flatten();

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
    /// use shared::{event::*, lookup::*};
    /// let plain_key = "lick";
    /// let lookup_key = LookupBuf::from_str("vic.stick.slam").unwrap();
    /// let mut value = Value::from(std::collections::BTreeMap::default());
    /// value.insert(plain_key, 1);
    /// value.insert(lookup_key, 2);
    ///
    /// let mut keys = value.pairs(None, false);
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
    /// let mut keys = value.pairs(None, true);
    /// assert_eq!(keys.next(), Some((Lookup::from_str("lick").unwrap(), &Value::from(1))));
    /// assert_eq!(keys.next(), Some((Lookup::from_str("vic.stick.slam").unwrap(), &Value::from(2))));
    /// ```
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
                let this = prefix
                    .clone()
                    .or_else(|| Some(Lookup::default()))
                    .map(|v| (v, self))
                    .into_iter();
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

                if only_leaves && !self.is_empty() {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
            Value::Array(a) => {
                trace!(prefix = ?prefix, "Enqueuing for iteration, may have children.");
                let this = prefix
                    .clone()
                    .or_else(|| Some(Lookup::default()))
                    .map(|v| (v, self))
                    .into_iter();
                let children = a
                    .iter()
                    .enumerate()
                    .map(move |(k, v)| {
                        let lookup = prefix.clone().map_or_else(
                            || Lookup::from(k as isize),
                            |mut l| {
                                l.push_back(Segment::index(k as isize));
                                l
                            },
                        );
                        trace!(lookup = ?lookup, "Seeking lookups inside non-leaf element.");
                        v.pairs(Some(lookup), only_leaves)
                    })
                    .flatten();

                if only_leaves && !self.is_empty() {
                    Box::new(children)
                } else {
                    Box::new(this.chain(children))
                }
            }
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
