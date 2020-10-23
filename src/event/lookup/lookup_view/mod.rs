#[cfg(test)]
mod test;

use super::{segmentbuf::SegmentBuf, LookupBuf};
use crate::event::lookup::Segment;
use crate::mapping::parser::{MappingParser, Rule};
use core::fmt;
use nom::lib::std::vec::IntoIter;
use pest::iterators::Pair;
use pest::Parser;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::ops::{Index, IndexMut};
use std::{
    convert::TryFrom,
    ops::{RangeFrom, RangeFull, RangeTo, RangeToInclusive},
    slice::Iter,
    str,
};

/// `Lookup`s are pre-validated event, unowned lookup paths.
///
/// These are unowned, ordered sets of segments. `Segment`s represent parts of a path such as
/// `pies.banana.slices[0]`. The segments would be `["pies", "banana", "slices", 0]`. You can "walk"
/// a lookup with an `iter()` call.
///
/// # Building
///
/// You build `Lookup`s from `str`s and other str-like objects with a `from()` or `try_from()`
/// call. **These do not parse the buffer.**
///
/// From there, you can `push` and `pop` onto the `Lookup`.
///
/// # Parsing
///
/// To parse buffer into a `Lookup`, use the `std::str::FromStr` implementation. If you're working
/// something that's not able to be a `str`, you should consult `std::str::from_utf8` and handle the
/// possible error.
///
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct Lookup<'a> {
    pub(super) segments: Vec<Segment<'a>>,
}

impl<'a> TryFrom<Pair<'a, Rule>> for Lookup<'a> {
    type Error = crate::Error;

    fn try_from(pair: Pair<'a, Rule>) -> Result<Self, Self::Error> {
        let retval = Self {
            segments: Segment::from_lookup(pair)?,
        };
        retval.is_valid()?;
        Ok(retval)
    }
}

impl<'a> Display for Lookup<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut peeker = self.segments.iter().peekable();
        let mut next = peeker.next();
        let mut maybe_next = peeker.peek();
        while let Some(segment) = next {
            match segment {
                Segment::Field(_) => match maybe_next {
                    Some(next) if next.is_field() => write!(f, r#"{}."#, segment)?,
                    None | Some(_) => write!(f, "{}", segment)?,
                },
                Segment::Index(_) => match maybe_next {
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

impl<'a> Lookup<'a> {
    /// Push onto the internal list of segments.
    #[instrument]
    pub fn push(&mut self, segment: Segment<'a>) {
        trace!(length = %self.segments.len(), "Pushing.");
        self.segments.push(segment)
    }

    #[instrument]
    pub fn pop(&mut self) -> Option<Segment<'a>> {
        trace!(length = %self.segments.len(), "Popping.");
        self.segments.pop()
    }

    #[instrument]
    pub fn iter(&self) -> Iter<'_, Segment<'a>> {
        self.segments.iter()
    }

    #[instrument]
    pub fn into_iter(self) -> IntoIter<Segment<'a>> {
        self.segments.into_iter()
    }

    /// Raise any errors that might stem from the lookup being invalid.
    #[instrument]
    pub fn is_valid(&self) -> crate::Result<()> {
        if self.segments.is_empty() {
            return Err("Lookups must have at least 1 segment to be valid.".into());
        }

        Ok(())
    }

    /// Parse the lookup from a str.
    #[instrument]
    pub fn from_str(input: &'a str) -> Result<Self, crate::Error> {
        let mut pairs = MappingParser::parse(Rule::lookup, input)?;
        let pair = pairs.next().ok_or("No tokens found.")?;
        Self::try_from(pair)
    }

    /// Become a `LookupBuf` (by allocating).
    #[instrument]
    pub fn into_buf(&self) -> LookupBuf {
        LookupBuf::from(self.segments)
    }
}

impl<'a> IntoIterator for Lookup<'a> {
    type Item = Segment<'a>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments.into_iter()
    }
}

impl<'a> From<&'a str> for Lookup<'a> {
    fn from(input: &'a str) -> Self {
        Self {
            segments: vec![Segment::field(input)],
        }
        // We know this must be at least one segment.
    }
}

impl<'a> TryFrom<Vec<Segment<'a>>> for Lookup<'a> {
    type Error = crate::Error;

    fn try_from(segments: Vec<Segment<'a>>) -> Result<Self, Self::Error> {
        let retval = Self { segments };
        retval.is_valid()?;
        Ok(retval)
    }
}

impl<'collection: 'item, 'item> TryFrom<&'collection [SegmentBuf]> for Lookup<'item> {
    type Error = crate::Error;

    fn try_from(segments: &'collection [SegmentBuf]) -> Result<Self, Self::Error> {
        let retval = Self {
            segments: segments.iter().map(Segment::from).collect(),
        };
        retval.is_valid()?;
        Ok(retval)
    }
}

impl<'a> From<&'a LookupBuf> for Lookup<'a> {
    fn from(lookup_buf: &'a LookupBuf) -> Self {
        Self::try_from(lookup_buf.segments.as_slice()).expect(
            "It is an invariant to have a 0 segment LookupBuf, so it is also an \
                     invariant to have a 0 segment Lookup.",
        )
    }
}

impl<'a> Index<usize> for Lookup<'a> {
    type Output = Segment<'a>;

    fn index(&self, index: usize) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> Index<RangeFull> for Lookup<'a> {
    type Output = [Segment<'a>];

    fn index(&self, index: RangeFull) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> Index<RangeToInclusive<usize>> for Lookup<'a> {
    type Output = [Segment<'a>];

    fn index(&self, index: RangeToInclusive<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> Index<RangeTo<usize>> for Lookup<'a> {
    type Output = [Segment<'a>];

    fn index(&self, index: RangeTo<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> Index<RangeFrom<usize>> for Lookup<'a> {
    type Output = [Segment<'a>];

    fn index(&self, index: RangeFrom<usize>) -> &Self::Output {
        self.segments.index(index)
    }
}

impl<'a> IndexMut<usize> for Lookup<'a> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.segments.index_mut(index)
    }
}

impl<'a> Serialize for Lookup<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&*self.to_string())
    }
}

impl<'de> Deserialize<'de> for Lookup<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Lookup<'de>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(LookupVisitor {
            _marker: Default::default(),
        })
    }
}

struct LookupVisitor<'a> {
    // This must exist to make the lifetime bounded.
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'de> Visitor<'de> for LookupVisitor<'de> {
    type Value = Lookup<'de>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Expected valid Lookup path.")
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Lookup::from_str(value).map_err(de::Error::custom)
    }
}

impl<'a> AsRef<Lookup<'a>> for Lookup<'a> {
    fn as_ref(&self) -> &Lookup<'a> {
        &self
    }
}