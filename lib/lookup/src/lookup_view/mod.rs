use core::fmt;
use std::{
    collections::VecDeque,
    fmt::{Display, Formatter},
    iter::IntoIterator,
    ops::{Index, IndexMut},
    str,
};

use inherent::inherent;
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};

use crate::{Look, LookupBuf, LookupError, SegmentBuf};

#[cfg(test)]
mod test;

mod segment;
pub use segment::{Field, Segment};

/// `Lookup`s are pre-validated event, unowned lookup paths.
///
/// These are unowned, ordered sets of segments. `Segment`s represent parts of a path such as
/// `pies.banana.slices[0]`. The segments would be `["pies", "banana", "slices", 0]`. You can "walk"
/// a lookup with an `iter()` call.
///
/// # Building
///
/// You build `Lookup`s from `str`s and other str-like objects with a `from()` call.
/// **These do not parse the buffer.**
///
/// ```rust
/// use lookup::Lookup;
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
/// use lookup::Lookup;
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
/// use lookup::Lookup;
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
        while let Some(segment) = peeker.next() {
            let maybe_next = peeker
                .peek()
                .map(|next| next.is_field() || next.is_coalesce())
                .unwrap_or(false);

            match (segment, maybe_next) {
                (Segment::Field(_), true) => write!(f, r#"{}."#, segment)?,
                (Segment::Field(_), false) => write!(f, "{}", segment)?,
                (Segment::Index(_), true) => write!(f, r#"[{}]."#, segment)?,
                (Segment::Index(_), false) => write!(f, "[{}]", segment)?,
                (Segment::Coalesce(_), true) => write!(f, r#"{}."#, segment)?,
                (Segment::Coalesce(_), false) => write!(f, "{}", segment)?,
            }
        }
        Ok(())
    }
}

impl<'a> Lookup<'a> {
    /// Creates a lookup to the root
    pub fn root() -> Self {
        Self {
            segments: VecDeque::new(),
        }
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, Segment<'a>> {
        self.segments.iter()
    }

    /// Become a `LookupBuf` (by allocating).
    pub fn into_buf(self) -> LookupBuf {
        LookupBuf::from(self)
    }
}

#[inherent]
impl<'a> Look<'a> for Lookup<'a> {
    type Segment = Segment<'a>;

    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn get(&mut self, index: usize) -> Option<&Segment<'a>> {
        self.segments.get(index)
    }

    pub fn push_back(&mut self, segment: impl Into<Segment<'a>>) {
        self.segments.push_back(segment.into())
    }

    pub fn pop_back(&mut self) -> Option<Segment<'a>> {
        self.segments.pop_back()
    }

    pub fn push_front(&mut self, segment: impl Into<Segment<'a>>) {
        self.segments.push_front(segment.into())
    }

    pub fn pop_front(&mut self) -> Option<Segment<'a>> {
        self.segments.pop_front()
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Parse the lookup from a str.
    #[allow(clippy::should_implement_trait)]
    // Cannot be defined as `FromStr` due to lifetime constraint on return type
    pub fn from_str(input: &'a str) -> Result<Self, LookupError> {
        crate::parser::parse_lookup(input).map_err(|err| LookupError::Invalid { message: err })
    }

    /// Merge a lookup.
    pub fn extend(&mut self, other: Self) {
        self.segments.extend(other.segments)
    }

    /// Returns `true` if `needle` is a prefix of the lookup.
    pub fn starts_with(&self, needle: &Lookup<'a>) -> bool {
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
    }
}

impl<'a> From<isize> for Lookup<'a> {
    fn from(input: isize) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(Segment::from(input));
        Self { segments }
    }
}

impl<'a> From<&'a String> for Lookup<'a> {
    fn from(input: &'a String) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(Segment::from(input.as_str()));
        Self { segments }
    }
}

impl<'a> From<Segment<'a>> for Lookup<'a> {
    fn from(input: Segment<'a>) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(input);
        Self { segments }
    }
}

impl<'a> From<VecDeque<Segment<'a>>> for Lookup<'a> {
    fn from(segments: VecDeque<Segment<'a>>) -> Self {
        Self { segments }
    }
}

impl<'collection: 'item, 'item> From<&'collection [SegmentBuf]> for Lookup<'item> {
    fn from(segments: &'collection [SegmentBuf]) -> Self {
        Self {
            segments: segments.iter().map(Segment::from).collect(),
        }
    }
}

impl<'collection: 'item, 'item> From<&'collection VecDeque<SegmentBuf>> for Lookup<'item> {
    fn from(segments: &'collection VecDeque<SegmentBuf>) -> Self {
        Self {
            segments: segments.iter().map(Segment::from).collect(),
        }
    }
}

impl<'a> From<Field<'a>> for Lookup<'a> {
    fn from(field: Field<'a>) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(Segment::Field(field));
        Self { segments }
    }
}

impl<'a> From<&'a LookupBuf> for Lookup<'a> {
    fn from(lookup_buf: &'a LookupBuf) -> Self {
        Self::from(&lookup_buf.segments)
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
        serializer.serialize_str(&self.to_string())
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
        self
    }
}
