use std::collections::VecDeque;
use std::fmt::{Debug, Display};
use std::hash::Hash;

pub use error::LookupError;
pub use lookup_buf::{LookupBuf, SegmentBuf};
pub use lookup_view::{Lookup, Segment};

mod error;
mod lookup_buf;
mod lookup_view;

// This trait, while it is not necessarily imported and used, exists
// to enforce parity among view/buf types.
//
// It is convention to implement these functions on the bare type itself,
// then have the implementation proxy to this **without modification**.
//
// This is so the functions are always available to users, without needing an import.
trait Look<'a>:
    Debug + Display + PartialEq + Eq + PartialOrd + Ord + Clone + Hash + Sized + ToString
{
    type Segment: LookSegment<'a>;

    fn get(&mut self, index: usize) -> Option<&Self::Segment>;

    fn push_back(&mut self, segment: impl Into<Self::Segment>);

    fn pop_back(&mut self) -> Option<Self::Segment>;

    fn push_front(&mut self, segment: impl Into<Self::Segment>);

    fn pop_front(&mut self) -> Option<Self::Segment>;

    fn len(&self) -> usize;

    fn is_valid(&self) -> Result<(), LookupError>;

    fn from_str(input: &'a str) -> Result<Self, LookupError>;

    fn extend(&mut self, other: Self);

    fn starts_with(&self, needle: &Self) -> bool;
}

// It is convention to only proxy to the struct implementations **without modification**.
// This is so the functions are always available to users, but we are required to expose the same API.
impl<'a> Look<'a> for Lookup<'a> {
    type Segment = Segment<'a>;

    fn get(&mut self, index: usize) -> Option<&Self::Segment> {
        Lookup::get(self, index)
    }
    fn push_back(&mut self, segment: impl Into<Self::Segment>) {
        Lookup::push_back(self, segment)
    }
    fn pop_back(&mut self) -> Option<Self::Segment> {
        Lookup::pop_back(self)
    }
    fn push_front(&mut self, segment: impl Into<Self::Segment>) {
        Lookup::push_front(self, segment)
    }
    fn pop_front(&mut self) -> Option<Self::Segment> {
        Lookup::pop_front(self)
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn is_valid(&self) -> Result<(), LookupError> {
        Lookup::is_valid(self)
    }
    fn from_str(input: &'a str) -> Result<Self, LookupError> {
        Lookup::from_str(input)
    }
    fn extend(&mut self, other: Self) {
        Lookup::extend(self, other)
    }
    fn starts_with(&self, needle: &Self) -> bool {
        Lookup::starts_with(self, needle)
    }
}

// It is convention to only proxy to the struct implementations **without modification**.
// This is so the functions are always available to users, but we are required to expose the same API.
impl Look<'static> for LookupBuf {
    type Segment = SegmentBuf;

    fn get(&mut self, index: usize) -> Option<&Self::Segment> {
        LookupBuf::get(self, index)
    }
    fn push_back(&mut self, segment: impl Into<Self::Segment>) {
        LookupBuf::push_back(self, segment)
    }
    fn pop_back(&mut self) -> Option<Self::Segment> {
        LookupBuf::pop_back(self)
    }
    fn push_front(&mut self, segment: impl Into<Self::Segment>) {
        LookupBuf::push_front(self, segment)
    }
    fn pop_front(&mut self) -> Option<Self::Segment> {
        LookupBuf::pop_front(self)
    }
    fn len(&self) -> usize {
        self.len()
    }
    fn is_valid(&self) -> Result<(), LookupError> {
        LookupBuf::is_valid(self)
    }
    fn from_str(input: &'static str) -> Result<Self, LookupError> {
        LookupBuf::from_str(input)
    }
    fn extend(&mut self, other: Self) {
        LookupBuf::extend(self, other)
    }
    fn starts_with(&self, needle: &Self) -> bool {
        LookupBuf::starts_with(self, needle)
    }
}

// This trait, while it is not necessarily imported and used, exists
// to enforce parity among view/buf types.
//
// It is convention to implement these functions on the bare type itself,
// then have the implementation proxy to this **without modification**.
//
// This is so the functions are always available to users, without needing an import.
trait LookSegment<'a>: Debug + PartialEq + Eq + PartialOrd + Ord + Clone + Hash + Sized {
    type Field: Debug + PartialEq + Eq + PartialOrd + Ord + Clone + Hash + Sized;

    fn field(name: Self::Field, requires_quoting: bool) -> Self;

    fn is_field(&self) -> bool;

    fn index(v: isize) -> Self;

    fn is_index(&self) -> bool;

    fn coalesce(v: Vec<VecDeque<Self>>) -> Self;

    fn is_coalesce(&self) -> bool;
}

// It is convention to only proxy to the struct implementations **without modification**.
// This is so the functions are always available to users, but we are required to expose the same API.
impl<'a> LookSegment<'a> for SegmentBuf {
    type Field = String;
    fn field(name: <Self as LookSegment>::Field, requires_quoting: bool) -> Self {
        Self::field(name, requires_quoting)
    }
    fn is_field(&self) -> bool {
        self.is_field()
    }
    fn index(v: isize) -> Self {
        Self::index(v)
    }
    fn is_index(&self) -> bool {
        self.is_index()
    }
    fn coalesce(v: Vec<VecDeque<Self>>) -> Self {
        Self::coalesce(v)
    }
    fn is_coalesce(&self) -> bool {
        self.is_coalesce()
    }
}

// It is convention to only proxy to the struct implementations **without modification**.
// This is so the functions are always available to users, but we are required to expose the same API.
impl<'a> LookSegment<'a> for Segment<'a> {
    type Field = &'a str;
    fn field(name: <Self as LookSegment<'a>>::Field, requires_quoting: bool) -> Self {
        Self::field(name, requires_quoting)
    }
    fn is_field(&self) -> bool {
        self.is_field()
    }
    fn index(v: isize) -> Self {
        Self::index(v)
    }
    fn is_index(&self) -> bool {
        self.is_index()
    }
    fn coalesce(v: Vec<VecDeque<Self>>) -> Self {
        Self::coalesce(v)
    }
    fn is_coalesce(&self) -> bool {
        self.is_coalesce()
    }
}
