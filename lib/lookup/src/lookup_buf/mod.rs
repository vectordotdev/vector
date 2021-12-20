use std::{
    borrow::Cow,
    collections::VecDeque,
    fmt::{self, Display, Formatter},
    ops::{Index, IndexMut},
    str,
    str::FromStr,
};

use inherent::inherent;
#[cfg(any(test, feature = "arbitrary"))]
use quickcheck::{Arbitrary, Gen};
use serde::{
    de::{self, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};

use crate::{Look, Lookup, LookupError};

#[cfg(test)]
mod test;

mod segmentbuf;
pub use segmentbuf::{FieldBuf, SegmentBuf};

/// `LookupBuf`s are pre-validated, owned event lookup paths.
///
/// These are owned, ordered sets of `SegmentBuf`s. `SegmentBuf`s represent parts of a path such as
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
/// ```rust
/// use lookup::LookupBuf;
/// let mut lookup = LookupBuf::from("foo");
/// lookup.push_back(1);
/// lookup.push_back("bar");
///
/// let mut lookup = LookupBuf::from("foo.bar"); // This is **not** two segments.
/// lookup.push_back(1);
/// lookup.push_back("bar");
/// ```
///
/// # Parsing
///
/// to parse buffer into a `LookupBuf`, use the `std::str::FromStr` implementation. If you're working
/// something that's not able to be a `str`, you should consult `std::str::from_utf8` and handle the
/// possible error.
///
/// ```rust
/// use lookup::LookupBuf;
/// let mut lookup = LookupBuf::from_str("foo").unwrap();
/// lookup.push_back(1);
/// lookup.push_back("bar");
///
/// let mut lookup = LookupBuf::from_str("foo.bar").unwrap(); // This **is** two segments.
/// lookup.push_back(1);
/// lookup.push_back("bar");
/// ```
///
/// # Unowned Variant
///
/// There exists an unowned variant of this type appropriate for static contexts or where you only
/// have a view into a long lived string. (Say, deserialization of configs).
///
/// To shed ownership use `lookup_buf.to_lookup()`. To gain ownership of a `lookup`, use
/// `lookup.into()`.
///
/// ```rust
/// use lookup::LookupBuf;
/// let mut lookup = LookupBuf::from_str("foo.bar").unwrap();
/// let mut unowned_view = lookup.to_lookup();
/// unowned_view.push_back(1);
/// unowned_view.push_back("bar");
/// lookup.push_back("baz"); // Does not impact the view!
/// ```
///
/// For more, investigate `Lookup`.
#[derive(Debug, PartialEq, Default, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct LookupBuf {
    pub segments: VecDeque<SegmentBuf>,
}

#[cfg(any(test, feature = "arbitrary"))]
impl Arbitrary for LookupBuf {
    fn arbitrary(g: &mut Gen) -> Self {
        LookupBuf {
            segments: {
                // Limit the number of segments generated to a fairly realistic number,
                // otherwise the tests take ages to run and don't add any extra value.
                let size = usize::arbitrary(g) % 20 + 1;
                (0..size).map(|_| SegmentBuf::arbitrary(g)).collect()
            },
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(
            self.segments
                .shrink()
                .filter(|segments| !segments.is_empty())
                .map(|segments| Self { segments }),
        )
    }
}

impl Display for LookupBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let mut peeker = self.segments.iter().peekable();
        while let Some(segment) = peeker.next() {
            let maybe_next = peeker
                .peek()
                .map(|next| next.is_field() || next.is_coalesce())
                .unwrap_or(false);
            match (segment, maybe_next) {
                (SegmentBuf::Field(_), true) => write!(f, r#"{}."#, segment)?,
                (SegmentBuf::Field(_), false) => write!(f, "{}", segment)?,
                (SegmentBuf::Index(_), true) => write!(f, r#"[{}]."#, segment)?,
                (SegmentBuf::Index(_), false) => write!(f, "[{}]", segment)?,
                (SegmentBuf::Coalesce(_), true) => write!(f, r#"{}."#, segment)?,
                (SegmentBuf::Coalesce(_), false) => write!(f, "{}", segment)?,
            }
        }
        Ok(())
    }
}

impl LookupBuf {
    /// Creates a lookup to the root
    pub fn root() -> Self {
        Self {
            segments: VecDeque::new(),
        }
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, SegmentBuf> {
        self.segments.iter()
    }

    pub fn to_lookup(&self) -> Lookup {
        Lookup::from(self)
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn from_segments(segments: Vec<SegmentBuf>) -> Self {
        Self {
            segments: segments.into_iter().collect(),
        }
    }

    /// Return a borrow of the SegmentBuf set.
    pub fn as_segments(&self) -> &VecDeque<SegmentBuf> {
        &self.segments
    }

    /// Create the possible fields that can be followed by this lookup.
    /// Because of coalesced paths there can be a number of different combinations.
    /// There is the potential for this function to create a vast number of different
    /// combinations if there are multiple coalesced segments in a path.
    ///
    /// The limit specifies the limit of the path depth we are interested in.
    /// Metrics is only interested in fields that are up to 3 levels deep (2 levels + 1 to check it
    /// terminates).
    ///
    /// eg, .tags.nork.noog will never be an accepted path so we don't need to spend the time
    /// collecting it.
    pub fn to_alternative_components(&self, limit: usize) -> Vec<Vec<&str>> {
        let mut components = vec![vec![]];
        for segment in self.segments.iter().take(limit) {
            match segment {
                SegmentBuf::Field(FieldBuf { name, .. }) => {
                    for component in &mut components {
                        component.push(name.as_str());
                    }
                }

                SegmentBuf::Coalesce(fields) => {
                    components = components
                        .iter()
                        .flat_map(|path| {
                            fields.iter().map(move |field| {
                                let mut path = path.clone();
                                path.push(field.name.as_str());
                                path
                            })
                        })
                        .collect();
                }

                SegmentBuf::Index(_) => {
                    return Vec::new();
                }
            }
        }

        components
    }
}

#[inherent]
impl Look<'static> for LookupBuf {
    type Segment = SegmentBuf;

    /// Get from the internal list of segments.
    pub fn get(&mut self, index: usize) -> Option<&SegmentBuf> {
        self.segments.get(index)
    }

    /// Push onto the internal list of segments.
    pub fn push_back(&mut self, segment: impl Into<SegmentBuf>) {
        self.segments.push_back(segment.into());
    }

    pub fn pop_back(&mut self) -> Option<SegmentBuf> {
        self.segments.pop_back()
    }

    pub fn push_front(&mut self, segment: impl Into<SegmentBuf>) {
        self.segments.push_front(segment.into())
    }

    pub fn pop_front(&mut self) -> Option<SegmentBuf> {
        self.segments.pop_front()
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_root(&self) -> bool {
        self.is_empty()
    }

    #[allow(clippy::should_implement_trait)]
    // This is also defined as `FromStr` on `LookupBuf` but we need `from_str` to be defined on the
    // `Lookup` trait itself since we cannot define `FromStr` for `LookupView` due to the lifetime
    // constraint
    pub fn from_str(value: &'static str) -> Result<LookupBuf, LookupError> {
        Lookup::from_str(value).map(|l| l.into_buf())
    }

    /// Merge a lookup.
    pub fn extend(&mut self, other: Self) {
        self.segments.extend(other.segments)
    }

    /// Returns `true` if `needle` is a prefix of the lookup.
    pub fn starts_with(&self, needle: &LookupBuf) -> bool {
        needle.iter().zip(&self.segments).all(|(n, s)| n == s)
    }
}

impl FromStr for LookupBuf {
    type Err = LookupError;

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

impl From<VecDeque<SegmentBuf>> for LookupBuf {
    fn from(segments: VecDeque<SegmentBuf>) -> Self {
        LookupBuf { segments }
    }
}

impl From<String> for LookupBuf {
    fn from(input: String) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(SegmentBuf::from(input));
        LookupBuf { segments }
    }
}

impl From<Cow<'_, str>> for LookupBuf {
    fn from(input: Cow<'_, str>) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(SegmentBuf::from(input.as_ref()));
        LookupBuf { segments }
    }
}

impl From<SegmentBuf> for LookupBuf {
    fn from(input: SegmentBuf) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(input);
        LookupBuf { segments }
    }
}

impl From<isize> for LookupBuf {
    fn from(input: isize) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(SegmentBuf::index(input));
        LookupBuf { segments }
    }
}

impl From<&str> for LookupBuf {
    fn from(input: &str) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(SegmentBuf::from(input.to_owned()));
        LookupBuf { segments }
    }
}

impl From<FieldBuf> for LookupBuf {
    fn from(field: FieldBuf) -> Self {
        let mut segments = VecDeque::with_capacity(1);
        segments.push_back(SegmentBuf::Field(field));
        Self { segments }
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
        serializer.serialize_str(&*ToString::to_string(self))
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
        FromStr::from_str(value).map_err(de::Error::custom)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        FromStr::from_str(&value).map_err(de::Error::custom)
    }
}

impl<'a> From<Lookup<'a>> for LookupBuf {
    fn from(v: Lookup<'a>) -> Self {
        let segments = v
            .segments
            .into_iter()
            .map(|f| f.as_segment_buf())
            .collect::<VecDeque<_>>();
        LookupBuf::from(segments)
    }
}
