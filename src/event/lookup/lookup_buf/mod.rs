#![allow(clippy::len_without_is_empty)] // It's invalid to have a lookupbuf that is empty.

use crate::event::Value;
use pest::iterators::Pair;
use remap::parser::ParserRule;
use std::{
    collections::VecDeque,
    convert::TryFrom,
    ops::{RangeFrom, RangeFull, RangeTo, RangeToInclusive},
    slice::Iter,
    str,
    fmt::{self, Display, Formatter},
    ops::{Index, IndexMut},
    str::FromStr,
};
use toml::Value as TomlValue;

use super::{segmentbuf::SegmentBuf, Lookup};
use crate::event::lookup::Segment;
use indexmap::map::IndexMap;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
    pub(super) segments: VecDeque<SegmentBuf>,
}

impl<'a> TryFrom<Pair<'a, ParserRule>> for LookupBuf {
    type Error = crate::Error;

    fn try_from(pair: Pair<'a, ParserRule>) -> Result<Self, Self::Error> {
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

impl<'a> TryFrom<VecDeque<SegmentBuf>> for LookupBuf {
    type Error = crate::Error;

    fn try_from(segments: VecDeque<SegmentBuf>) -> Result<Self, Self::Error> {
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
                SegmentBuf::Field {
                    name: _,
                    requires_quoting: _,
                } => match maybe_next {
                    Some(next) if next.is_field() => write!(f, r#"{}."#, segment)?,
                    None | Some(_) => write!(f, "{}", segment)?,
                },
                SegmentBuf::Index(_) => match maybe_next {
                    Some(next) if next.is_field() => write!(f, r#"[{}]."#, segment)?,
                    None | Some(_) => write!(f, "[{}]", segment)?,
                },
                SegmentBuf::Coalesce(_) => unimplemented!(),
            }
            next = peeker.next();
            maybe_next = peeker.peek();
        }
        Ok(())
    }
}

impl LookupBuf {
    /// Push onto the internal list of segments.
    #[instrument(level = "trace")]
    pub fn push_back(&mut self, segment: SegmentBuf) {
        trace!(length = %self.segments.len(), "Pushing.");
        self.segments.push_back(segment);
    }

    #[instrument(level = "trace")]
    pub fn pop_back(&mut self) -> Option<SegmentBuf> {
        trace!(length = %self.segments.len(), "Popping.");
        self.segments.pop_back()
    }

    #[instrument(level = "trace")]
    pub fn push_front(&mut self, segment: SegmentBuf) {
        trace!(length = %self.segments.len(), "Pushing.");
        self.segments.push_front(segment)
    }

    #[instrument(level = "trace")]
    pub fn pop_front(&mut self) -> Option<SegmentBuf> {
        trace!(length = %self.segments.len(), "Popping.");
        self.segments.pop_front()
    }


    #[instrument(level = "trace")]
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, SegmentBuf> {
        self.segments.iter()
    }

    #[instrument(level = "trace")]
    pub fn from_indexmap(
        values: IndexMap<String, TomlValue>,
    ) -> crate::Result<IndexMap<LookupBuf, Value>> {
        let mut discoveries = IndexMap::new();
        for (key, value) in values {
            Self::from_toml_table_recursive_step(
                LookupBuf::try_from(key)?,
                value,
                &mut discoveries,
            )?;
        }
        Ok(discoveries)
    }

    #[instrument(level = "trace")]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    #[instrument(level = "trace")]
    pub fn from_toml_table(value: TomlValue) -> crate::Result<IndexMap<LookupBuf, Value>> {
        let mut discoveries = IndexMap::new();
        match value {
            TomlValue::Table(map) => {
                for (key, value) in map {
                    Self::from_toml_table_recursive_step(
                        LookupBuf::try_from(key)?,
                        value,
                        &mut discoveries,
                    )?;
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

    #[instrument(level = "trace")]
    fn from_toml_table_recursive_step(
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
                    Self::from_toml_table_recursive_step(
                        LookupBuf::try_from(key)?,
                        val,
                        discoveries,
                    )?;
                }
                None
            }
            TomlValue::Table(map) => {
                for (table_key, value) in map {
                    let key = format!("{}.{}", lookup, table_key);
                    Self::from_toml_table_recursive_step(
                        LookupBuf::try_from(key)?,
                        value,
                        discoveries,
                    )?;
                }
                None
            }
        };
        Ok(())
    }

    /// Raise any errors that might stem from the lookup being invalid.
    #[instrument(level = "trace")]
    pub fn is_valid(&self) -> crate::Result<()> {
        if self.segments.is_empty() {
            return Err("Lookups must have at least 1 segment to be valid.".into());
        }

        Ok(())
    }

    #[instrument(level = "trace")]
    pub fn clone_lookup(&self) -> Lookup {
        Lookup::from(self)
    }

    #[instrument(level = "trace")]
    pub fn from_str(value: &str) -> Result<LookupBuf, crate::Error> {
        Lookup::from_str(value).map(|l| l.into_buf())
    }

    /// Return a borrow of the SegmentBuf set.
    #[instrument(level = "trace")]
    pub fn as_segments(&self) -> &VecDeque<SegmentBuf> {
        &self.segments
    }

    /// Return the SegmentBuf set.
    #[instrument(level = "trace")]
    pub fn into_segments(self) -> VecDeque<SegmentBuf> {
        self.segments
    }

    /// Merge a lookup.
    #[instrument(level = "trace")]
    pub fn extend(&mut self, other: Self) {
        self.segments.extend(other.segments)
    }

    /// Returns `true` if `needle` is a prefix of the lookup.
    #[instrument(level = "trace")]
    pub fn starts_with(&self, needle: &LookupBuf) -> bool {
        needle.iter().zip(&self.segments).all(|(n, s)| n == s )
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
    type IntoIter = std::collections::vec_deque::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments.into_iter()
    }
}

impl From<String> for LookupBuf {
    fn from(input: String) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(SegmentBuf::from(input));
        LookupBuf {
            segments,
        }
        // We know this must be at least one segment.
    }
}

impl From<usize> for LookupBuf {
    fn from(input: usize) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(SegmentBuf::index(input));
        LookupBuf {
            segments,
        }
        // We know this must be at least one segment.
    }
}

impl From<&str> for LookupBuf {
    fn from(input: &str) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(SegmentBuf::from(input.to_owned()));
        LookupBuf {
            segments,
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
        deserializer.deserialize_string(LookupBufVisitor)
    }
}

struct LookupBufVisitor;

impl<'de> Visitor<'de> for LookupBufVisitor {
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
            .collect::<VecDeque<_>>();
        let retval: Result<LookupBuf, crate::Error> = LookupBuf::try_from(segments);
        retval.expect(
            "A LookupBuf with 0 length was turned into a Lookup. Since a LookupBuf with 0 \
                  length is an invariant, any action on it is too.",
        )
    }
}
