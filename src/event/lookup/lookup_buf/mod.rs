use crate::{event::Value, mapping::parser::Rule};
use pest::iterators::Pair;
use std::{
    convert::TryFrom,
    ops::{RangeFrom, RangeFull, RangeTo, RangeToInclusive},
    slice::Iter,
    str,
};
use toml::Value as TomlValue;

use super::{segmentbuf::SegmentBuf, Lookup};
use crate::event::lookup::Segment;
use core::fmt;
use indexmap::map::IndexMap;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::ops::{Index, IndexMut};
use std::str::FromStr;

#[cfg(test)]
mod test;

/// `LookupBuf`s are pre-validated, owned event lookup paths.
///
/// These are owned, ordered sets of `Segment`s. `Segment`s represent parts of a path such as
/// `pies.banana.slices[0]`. The segments would be `["pies", "banana", "slices", 0]`. You can "walk"
/// a `LookupBuf` with an `iter()` call.
///
/// # Building
///
/// You build `LookupBuf`s from `String`s and other string-like objects with a `from()` or `try_from()`
/// call. **These do not parse the buffer.**
///
/// From there, you can `push` and `pop` onto the `LookupBuf`.
///
/// # Parsing
///
/// To parse buffer into a `LookupBuf`, use the `std::str::FromStr` implementation. If you're working
/// something that's not able to be a `str`, you should consult `std::str::from_utf8` and handle the
/// possible error.
///
/// # Unowned Variant
///
/// There exists an unowned variant of this type appropriate for static contexts or where you only
/// have a view into a long lived string. (Say, deserialization of configs).
///
/// To shed ownership use `lookup_buf.as_lookup()`. To gain ownership of a `lookup`, use
/// `lookup.into()`.
///
/// For more, investigate `Lookup`.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct LookupBuf {
    pub(super) segments: Vec<SegmentBuf>,
}

impl<'a> TryFrom<Pair<'a, Rule>> for LookupBuf {
    type Error = crate::Error;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        let retval = LookupBuf {
            segments: Segment::from_lookup(pair)?
                .into_iter()
                .map(Into::into)
                .collect(),
        };
        retval.is_valid()?;
        Ok(retval)
    }
}

impl<'a> TryFrom<Vec<SegmentBuf>> for LookupBuf {
    type Error = crate::Error;

    fn try_from(segments: Vec<SegmentBuf>) -> Result<Self, Self::Error> {
        let retval = LookupBuf { segments };
        retval.is_valid()?;
        Ok(retval)
    }
}

impl Display for LookupBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut peeker = self.segments.iter().peekable();
        let mut next = peeker.next();
        let mut maybe_next = peeker.peek();
        while let Some(segment) = next {
            match segment {
                SegmentBuf::Field(_) => match maybe_next {
                    Some(next) if next.is_field() => write!(f, r#"{}."#, segment)?,
                    None | Some(_) => write!(f, "{}", segment)?,
                },
                SegmentBuf::Index(_) => match maybe_next {
                    Some(next) if next.is_field() => write!(f, r#"[{}]."#, segment)?,
                    None | Some(_) => write!(f, "[{}]", segment)?,
                },
            }
            next = peeker.next();
            maybe_next = peeker.peek();
        }
        Ok(())
    }
}

impl LookupBuf {
    /// Push onto the internal list of segments.
    #[instrument]
    pub fn push(&mut self, segment: SegmentBuf) {
        trace!(length = %self.segments.len(), "Pushing.");
        self.segments.push(segment);
    }

    #[instrument]
    pub fn pop(&mut self) -> Option<SegmentBuf> {
        trace!(length = %self.segments.len(), "Popping.");
        self.segments.pop()
    }

    #[instrument]
    pub fn iter(&self) -> Iter<'_, SegmentBuf> {
        self.segments.iter()
    }

    #[instrument]
    pub fn from_indexmap(
        values: IndexMap<String, TomlValue>,
    ) -> crate::Result<IndexMap<LookupBuf, Value>> {
        let mut discoveries = IndexMap::new();
        for (key, value) in values {
            Self::recursive_step(LookupBuf::try_from(key)?, value, &mut discoveries)?;
        }
        Ok(discoveries)
    }

    #[instrument]
    pub fn from_toml_table(value: TomlValue) -> crate::Result<IndexMap<LookupBuf, Value>> {
        let mut discoveries = IndexMap::new();
        match value {
            TomlValue::Table(map) => {
                for (key, value) in map {
                    Self::recursive_step(LookupBuf::try_from(key)?, value, &mut discoveries)?;
                }
                Ok(discoveries)
            }
            _ => Err(format!(
                "A TOML table must be passed to the `from_toml_table` function. Passed: {:?}",
                value
            )
            .into()),
        }
    }

    #[instrument]
    fn recursive_step(
        lookup: LookupBuf,
        value: TomlValue,
        discoveries: &mut IndexMap<LookupBuf, Value>,
    ) -> crate::Result<()> {
        match value {
            TomlValue::String(s) => discoveries.insert(lookup, Value::from(s)),
            TomlValue::Integer(i) => discoveries.insert(lookup, Value::from(i)),
            TomlValue::Float(f) => discoveries.insert(lookup, Value::from(f)),
            TomlValue::Boolean(b) => discoveries.insert(lookup, Value::from(b)),
            TomlValue::Datetime(dt) => {
                let dt = dt.to_string();
                discoveries.insert(lookup, Value::from(dt))
            }
            TomlValue::Array(vals) => {
                for (i, val) in vals.into_iter().enumerate() {
                    let key = format!("{}[{}]", lookup, i);
                    Self::recursive_step(LookupBuf::try_from(key)?, val, discoveries)?;
                }
                None
            }
            TomlValue::Table(map) => {
                for (table_key, value) in map {
                    let key = format!("{}.{}", lookup, table_key);
                    Self::recursive_step(LookupBuf::try_from(key)?, value, discoveries)?;
                }
                None
            }
        };
        Ok(())
    }

    /// Raise any errors that might stem from the lookup being invalid.
    #[instrument]
    pub fn is_valid(&self) -> crate::Result<()> {
        if self.segments.is_empty() {
            return Err("Lookups must have at least 1 segment to be valid.".into());
        }

        Ok(())
    }

    #[instrument]
    pub fn as_lookup(&self) -> Lookup<'_> {
        Lookup::from(self)
    }

    #[instrument]
    pub fn from_str(value: &str) -> LookupBuf {
        Lookup::from(value).into_buf()
    }
}

impl FromStr for LookupBuf {
    type Err = crate::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let lookup = Lookup::from_str(input)?;
        let lookup_buf: LookupBuf = lookup.into();
        Ok(lookup_buf)
    }
}

impl IntoIterator for LookupBuf {
    type Item = SegmentBuf;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments.into_iter()
    }
}

impl From<String> for LookupBuf {
    fn from(input: String) -> Self {
        LookupBuf {
            segments: vec![SegmentBuf::field(input)],
        }
        // We know this must be at least one segment.
    }
}

impl From<&str> for LookupBuf {
    fn from(input: &str) -> Self {
        LookupBuf {
            segments: vec![SegmentBuf::field(input.to_owned())],
        }
        // We know this must be at least one segment.
    }
}

impl Index<usize> for LookupBuf {
    type Output = SegmentBuf;

    fn index(&self, index: usize) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeFull> for LookupBuf {
    type Output = [SegmentBuf];

    fn index(&self, index: RangeFull) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeToInclusive<usize>> for LookupBuf {
    type Output = [SegmentBuf];

    fn index(&self, index: RangeToInclusive<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeTo<usize>> for LookupBuf {
    type Output = [SegmentBuf];

    fn index(&self, index: RangeTo<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl Index<RangeFrom<usize>> for LookupBuf {
    type Output = [SegmentBuf];

    fn index(&self, index: RangeFrom<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl IndexMut<usize> for LookupBuf {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.segments.index_mut(index)
    }
}

impl Serialize for LookupBuf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&*self.to_string())
    }
}

impl<'de> Deserialize<'de> for LookupBuf {
    fn deserialize<D>(deserializer: D) -> Result<LookupBuf, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(LookupVisitor)
    }
}

struct LookupVisitor;

impl<'de> Visitor<'de> for LookupVisitor {
    type Value = LookupBuf;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Expected valid Lookup path.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        LookupBuf::from_str(value).map_err(de::Error::custom)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        LookupBuf::from_str(&value).map_err(de::Error::custom)
    }
}

impl<'a> From<Lookup<'a>> for LookupBuf {
    fn from(v: Lookup<'a>) -> Self {
        let segments = v
            .segments
            .into_iter()
            .map(|f| f.as_segment_buf())
            .collect::<Vec<_>>();
        let retval: Result<LookupBuf, crate::Error> = LookupBuf::try_from(segments);
        retval.expect(
            "A LookupBuf with 0 length was turned into a Lookup. Since a LookupBuf with 0 \
                  length is an invariant, any action on it is too.",
        )
    }
}

impl<'buf: 'view, 'view> AsRef<Lookup<'view>> for &'buf LookupBuf {
    fn as_ref(&self) -> &Lookup<'view> {
        &self.as_lookup()
    }
}

impl<'buf: 'view, 'view> std::borrow::Borrow<Lookup<'view>> for &'buf LookupBuf {
    fn borrow(&self) -> &Lookup<'view> {
        self.as_lookup()
    }
}
