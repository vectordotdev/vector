mod segmentbuf;
pub use segmentbuf::SegmentBuf;

mod lookup_view;
pub use lookup_view::Lookup;
mod lookup_buf;
pub use lookup_buf::LookupBuf;
mod segment;
pub use segment::Segment;

use std::fmt::{Display, Debug};
use std::convert::TryFrom;
use pest::iterators::Pair;
use remap::parser::ParserRule;
use std::collections::VecDeque;
use std::str::FromStr;
use std::ops::{Index, IndexMut};
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use std::hash::Hash;

// This trait, while it is not necessarily imported and used, exists
// to enforce parity among view/buf types.
//
// It is convention to implement these functions on the bare type itself,
// then have the implementation proxy to this **without modification**.
//
// This is so the functions are always available to users.
trait Look<'a>:
    Debug +
    PartialEq +
    Eq +
    PartialOrd +
    Ord +
    Clone +
    Hash +
    Sized
{
    type Segment: LookSegment<'a>;

    fn get(&mut self, index: usize) -> Option<&Self::Segment>;

    fn push_back(&mut self, segment: Self::Segment);

    fn pop_back(&mut self) -> Option<Self::Segment>;

    fn push_front(&mut self, segment: Self::Segment);

    fn pop_front(&mut self) -> Option<Self::Segment>;

    fn len(&self) -> usize;

    fn iter(&self) -> std::collections::vec_deque::Iter<'_, Self::Segment>;

    fn into_iter(self) -> std::collections::vec_deque::IntoIter<Self::Segment>;

    fn is_valid(&self) -> crate::Result<()>;

    fn from_str(input: &'a str) -> Result<Self, crate::Error>;

    fn to_string(&self) -> String;

    fn extend(&mut self, other: Self);

    fn starts_with<'b>(&self, needle: &Self) -> bool;
}


// It is convention to only proxy to the struct implementations **without modification**.
// This is so the functions are always available to users, but we are required to expose the same API.
impl<'a> Look<'a> for Lookup<'a> {
    type Segment = Segment<'a>;

    fn get(&mut self, index: usize) -> Option<&Self::Segment> { self.get(index) }
    fn push_back(&mut self, segment: Self::Segment) { self.push_back(segment) }
    fn pop_back(&mut self) -> Option<Self::Segment> { self.pop_back() }
    fn push_front(&mut self, segment: Self::Segment) { self.push_front(segment) }
    fn pop_front(&mut self) -> Option<Self::Segment> { self.pop_front() }
    fn len(&self) -> usize { self.len() }
    fn iter(&self) -> std::collections::vec_deque::Iter<'_, Self::Segment> { self.iter() }
    fn into_iter(self) -> std::collections::vec_deque::IntoIter<Self::Segment> { IntoIterator::into_iter(self) }
    fn is_valid(&self) -> crate::Result<()> { self.is_valid() }
    fn from_str(input: &'a str) -> Result<Self, crate::Error> { Self::from_str(input) }
    fn to_string(&self) -> String { ToString::to_string(self) }
    fn extend(&mut self, other: Self) { self.extend(other) }
    fn starts_with<'b>(&self, needle: &Self) -> bool { self.starts_with(needle) }
}


// It is convention to only proxy to the struct implementations **without modification**.
// This is so the functions are always available to users, but we are required to expose the same API.
impl Look<'static> for LookupBuf {
    type Segment = SegmentBuf;

    fn get(&mut self, index: usize) -> Option<&Self::Segment> { self.get(index) }
    fn push_back(&mut self, segment: Self::Segment) { self.push_back(segment) }
    fn pop_back(&mut self) -> Option<Self::Segment> { self.pop_back() }
    fn push_front(&mut self, segment: Self::Segment) { self.push_front(segment) }
    fn pop_front(&mut self) -> Option<Self::Segment> { self.pop_front() }
    fn len(&self) -> usize { self.len() }
    fn iter(&self) -> std::collections::vec_deque::Iter<'_, Self::Segment> { self.iter() }
    fn into_iter(self) -> std::collections::vec_deque::IntoIter<Self::Segment> { IntoIterator::into_iter(self) }
    fn is_valid(&self) -> crate::Result<()> { self.is_valid() }
    fn from_str(input: &'static str) -> Result<Self, crate::Error> { Self::from_str(input) }
    fn to_string(&self) -> String { ToString::to_string(self) }
    fn extend(&mut self, other: Self) { self.extend(other) }
    fn starts_with<'b>(&self, needle: &Self) -> bool { self.starts_with(needle) }
}

// This trait, while it is not necessarily imported and used, exists
// to enforce parity among view/buf types.
//
// It is convention to implement these functions on the bare type itself,
// then have the implementation proxy to this **without modification**.
//
// This is so the functions are always available to users.
trait LookSegment<'a>:
    Debug +
    PartialEq +
    Eq +
    PartialOrd +
    Ord +
    Clone +
    Hash +
    Sized
{
    type Field: Debug + PartialEq + Eq + PartialOrd + Ord + Clone + Hash + Sized;

    fn field(name: Self::Field, requires_quoting: bool) -> Self;

    fn is_field(&self) -> bool;

    fn index(v: usize) -> Self;

    fn is_index(&self) -> bool;

    fn coalesce(v: Vec<VecDeque<Self>>) -> Self;

    fn is_coalesce(&self) -> bool;
}

// It is convention to only proxy to the struct implementations **without modification**.
// This is so the functions are always available to users, but we are required to expose the same API.
impl<'a> LookSegment<'a> for SegmentBuf {
    type Field = String;
    fn field(name: <Self as LookSegment>::Field, requires_quoting: bool) -> Self { Self::field(name, requires_quoting) }
    fn is_field(&self) -> bool { self.is_field() }
    fn index(v: usize) -> Self { Self::index(v) }
    fn is_index(&self) -> bool { self.is_index() }
    fn coalesce(v: Vec<VecDeque<Self>>) -> Self { Self::coalesce(v) }
    fn is_coalesce(&self) -> bool { self.is_coalesce() }
}

// It is convention to only proxy to the struct implementations **without modification**.
// This is so the functions are always available to users, but we are required to expose the same API.
impl<'a> LookSegment<'a> for Segment<'a> {
    type Field = &'a str;
    fn field(name: <Self as LookSegment>::Field, requires_quoting: bool) -> Self { Self::field(name, requires_quoting) }
    fn is_field(&self) -> bool { self.is_field() }
    fn index(v: usize) -> Self { Self::index(v) }
    fn is_index(&self) -> bool { self.is_index() }
    fn coalesce(v: Vec<VecDeque<Self>>) -> Self { Self::coalesce(v) }
    fn is_coalesce(&self) -> bool { self.is_coalesce() }
}
