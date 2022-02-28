use std::{
    fmt::{Debug, Display},
    hash::Hash,
};

pub use error::LookupError;
pub use lookup_buf::{FieldBuf, LookupBuf, SegmentBuf};
pub use lookup_view::{Field, Lookup, Segment};

mod error;
mod field;
mod lookup_buf;
mod lookup_view;
pub mod parser;

/// This trait, while it is not necessarily imported and used, exists
/// to enforce parity among view/buf types.
pub trait Look<'a>:
    Debug + Display + PartialEq + Eq + PartialOrd + Ord + Clone + Hash + Sized + ToString
{
    type Segment: LookSegment<'a>;

    fn get(&mut self, index: usize) -> Option<&Self::Segment>;

    fn push_back(&mut self, segment: impl Into<Self::Segment>);

    fn pop_back(&mut self) -> Option<Self::Segment>;

    fn push_front(&mut self, segment: impl Into<Self::Segment>);

    fn pop_front(&mut self) -> Option<Self::Segment>;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn from_str(input: &'a str) -> Result<Self, LookupError>;

    fn extend(&mut self, other: Self);

    fn starts_with(&self, needle: &Self) -> bool;

    fn is_root(&self) -> bool;
}

// This trait, while it is not necessarily imported and used, exists
// to enforce parity among view/buf types.
//
// It is convention to implement these functions on the bare type itself,
// then have the implementation proxy to this **without modification**.
//
// This is so the functions are always available to users, without needing an import.
pub trait LookSegment<'a>:
    Debug + PartialEq + Eq + PartialOrd + Ord + Clone + Hash + Sized
{
    type Field: Debug + PartialEq + Eq + PartialOrd + Ord + Clone + Hash + Sized;

    fn field(field: Self::Field) -> Self;

    fn is_field(&self) -> bool;

    fn index(v: isize) -> Self;

    fn is_index(&self) -> bool;

    fn coalesce(v: Vec<Self::Field>) -> Self;

    fn is_coalesce(&self) -> bool;
}
