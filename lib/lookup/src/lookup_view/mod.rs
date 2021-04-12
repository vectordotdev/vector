#![allow(clippy::len_without_is_empty)] // It's invalid to have a lookupbuf that is empty.

use crate::*;
use core::fmt;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::ops::{Index, IndexMut};
use std::{collections::VecDeque, convert::TryFrom, str};
use tracing::instrument;

#[cfg(test)]
mod test;

mod segment;
pub use segment::Segment;

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
/// ```rust
/// use shared::lookup::Lookup;
/// let mut lookup = Lookup::from("foo");
/// lookup.push_back(1);
/// lookup.push_back("bar");
///
/// let mut lookup = Lookup::from("foo.bar"); // This is **not** two segments.
/// lookup.push_back(1);
/// lookup.push_back("bar");
/// ```
///
/// From there, you can `push` and `pop` onto the `Lookup`.
///
/// # Parsing
///
/// To parse buffer into a `Lookup`, use the `std::str::FromStr` implementation. If you're working
/// something that's not able to be a `str`, you should consult `std::str::from_utf8` and handle the
/// possible error.
///
/// ```rust
/// use shared::lookup::Lookup;
/// let mut lookup = Lookup::from_str("foo").unwrap();
/// lookup.push_back(1);
/// lookup.push_back("bar");
///
/// let mut lookup = Lookup::from_str("foo.bar").unwrap(); // This **is** two segments.
/// lookup.push_back(1);
/// lookup.push_back("bar");
/// ```
///
/// # Owned Variant
///
/// There exists an owned variant of this type appropriate for more flexible contexts or where you
/// have a string. (Say, most of the time).
///
/// To shed ownership use `lookup_buf.into_buf()`. To gain ownership of a `lookup`, use
/// `lookup.into()`.
///
/// ```rust
/// use shared::lookup::Lookup;
/// let mut lookup = Lookup::from_str("foo.bar").unwrap();
/// let mut owned = lookup.clone().into_buf();
/// owned.push_back(1);
/// owned.push_back("bar");
/// lookup.push_back("baz"); // Does not impact the owned!
/// ```
///
/// # Warnings
///
/// * You **can not** deserialize lookups (that is, views, the buffers are fine) out of str slices
///   with escapes in serde_json. [serde_json does not allow it.](https://github.com/serde-rs/json/blob/master/src/read.rs#L424-L476)
///   You **must** use strings. This means it is **almost always not a good idea to deserialize a
///   string into a `Lookup`. **Use a `LookupBuf` instead.**
#[derive(Debug, PartialEq, Eq, Default, PartialOrd, Ord, Clone, Hash)]
pub struct Lookup<'a> {
    pub segments: VecDeque<Segment<'a>>,
}

impl<'a> Display for Lookup<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut peeker = self.segments.iter().peekable();
        let mut next = peeker.next();
        let mut maybe_next = peeker.peek();
        while let Some(segment) = next {
            match segment {
                Segment::Field {
                    name: _,
                    requires_quoting: _,
                } => match maybe_next {
                    Some(next) if next.is_field() || next.is_coalesce() => {
                        write!(f, r#"{}."#, segment)?
                    }
                    None | Some(_) => write!(f, "{}", segment)?,
                },
                Segment::Index(_) => match maybe_next {
                    Some(next) if next.is_field() || next.is_coalesce() => {
                        write!(f, r#"[{}]."#, segment)?
                    }
                    None | Some(_) => write!(f, "[{}]", segment)?,
                },
                Segment::Coalesce(_) => match maybe_next {
                    Some(next) if next.is_field() || next.is_coalesce() => {
                        write!(f, r#"{}."#, segment)?
                    }
                    None | Some(_) => write!(f, "{}", segment)?,
                },
            }
            next = peeker.next();
            maybe_next = peeker.peek();
        }
        Ok(())
    }
}

impl<'a> Lookup<'a> {
    #[instrument(level = "trace")]
    pub fn get(&mut self, index: usize) -> Option<&Segment<'a>> {
        self.segments.get(index)
    }

    #[instrument(level = "trace", skip(segment))]
    pub fn push_back(&mut self, segment: impl Into<Segment<'a>>) {
        self.segments.push_back(segment.into())
    }

    #[instrument(level = "trace")]
    pub fn pop_back(&mut self) -> Option<Segment<'a>> {
        self.segments.pop_back()
    }

    #[instrument(level = "trace", skip(segment))]
    pub fn push_front(&mut self, segment: impl Into<Segment<'a>>) {
        self.segments.push_front(segment.into())
    }

    #[instrument(level = "trace")]
    pub fn pop_front(&mut self) -> Option<Segment<'a>> {
        self.segments.pop_front()
    }

    #[instrument(level = "trace")]
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    #[instrument(level = "trace")]
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, Segment<'a>> {
        self.segments.iter()
    }

    #[instrument(level = "trace")]
    pub fn into_iter(self) -> std::collections::vec_deque::IntoIter<Segment<'a>> {
        self.segments.into_iter()
    }

    /// Raise any errors that might stem from the lookup being invalid.
    #[instrument(level = "trace")]
    pub fn is_valid(&self) -> Result<(), LookupError> {
        Ok(())
    }

    /// Parse the lookup from a str.
    #[instrument(level = "trace")]
    pub fn from_str(input: &'a str) -> Result<Self, LookupError> {
        todo!()
        //let mut pairs = RemapParser::parse(ParserRule::lookup, input)?;
        //let pair = pairs.next().ok_or(LookupError::NoTokens)?;
        //Self::try_from(pair)
    }

    /// Dump the value to a `String`.
    #[instrument(level = "trace")]
    pub fn to_string(&self) -> String {
        format!("{}", self)
    }

    /// Become a `LookupBuf` (by allocating).
    #[instrument(level = "trace")]
    pub fn into_buf(self) -> LookupBuf {
        LookupBuf::from(self)
    }

    /// Return a borrow of the Segment set.
    #[instrument(level = "trace")]
    pub fn as_segments(&self) -> &VecDeque<Segment<'_>> {
        &self.segments
    }

    /// Return the Segment set.
    #[instrument(level = "trace")]
    pub fn into_segments(self) -> VecDeque<Segment<'a>> {
        self.segments
    }

    /// Merge a lookup.
    #[instrument(level = "trace")]
    pub fn extend(&mut self, other: Self) {
        self.segments.extend(other.segments)
    }

    /// Returns `true` if `needle` is a prefix of the lookup.
    pub fn starts_with<'b>(&self, needle: &Lookup<'b>) -> bool {
        needle.iter().zip(&self.segments).all(|(n, s)| n == s)
    }
}

impl<'a> IntoIterator for Lookup<'a> {
    type Item = Segment<'a>;
    type IntoIter = std::collections::vec_deque::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.segments.into_iter()
    }
}

impl<'a> From<&'a str> for Lookup<'a> {
    fn from(input: &'a str) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(Segment::from(input));
        Self { segments }
        // We know this must be at least one segment.
    }
}

impl<'a> From<isize> for Lookup<'a> {
    fn from(input: isize) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(Segment::from(input));
        Self { segments }
        // We know this must be at least one segment.
    }
}

impl<'a> From<&'a String> for Lookup<'a> {
    fn from(input: &'a String) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(Segment::from(input.as_str()));
        Self { segments }
        // We know this must be at least one segment.
    }
}

impl<'a> TryFrom<VecDeque<Segment<'a>>> for Lookup<'a> {
    type Error = LookupError;

    fn try_from(segments: VecDeque<Segment<'a>>) -> Result<Self, Self::Error> {
        let retval = Self { segments };
        retval.is_valid()?;
        Ok(retval)
    }
}

impl<'collection: 'item, 'item> TryFrom<&'collection [SegmentBuf]> for Lookup<'item> {
    type Error = LookupError;

    fn try_from(segments: &'collection [SegmentBuf]) -> Result<Self, Self::Error> {
        let retval = Self {
            segments: segments.iter().map(Segment::from).collect(),
        };
        retval.is_valid()?;
        Ok(retval)
    }
}

impl<'collection: 'item, 'item> TryFrom<&'collection VecDeque<SegmentBuf>> for Lookup<'item> {
    type Error = LookupError;

    fn try_from(segments: &'collection VecDeque<SegmentBuf>) -> Result<Self, Self::Error> {
        let retval = Self {
            segments: segments.iter().map(Segment::from).collect(),
        };
        retval.is_valid()?;
        Ok(retval)
    }
}

impl<'a> From<&'a LookupBuf> for Lookup<'a> {
    fn from(lookup_buf: &'a LookupBuf) -> Self {
        Self::try_from(&lookup_buf.segments).expect(
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

/// **WARNING:**: You **can not** deserialize lookups (that is, views, the buffers
/// are fine) out of str slices with escapes. It's invalid. You **must** use lookupbufs.
struct LookupVisitor<'a> {
    // This must exist to make the lifetime bounded.
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'de> Visitor<'de> for LookupVisitor<'de> {
    type Value = Lookup<'de>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter
            .write_str("Expected valid Lookup path. If deserializing a string, use a LookupBuf.")
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
