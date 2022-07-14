mod borrowed;
mod compat;
mod concat;
mod jit;
mod owned;
mod simple;

use self::jit::{JitLookup, JitPath};

use crate::lookup_v2::simple::SimpleSegmentIter;
pub use borrowed::BorrowedSegment;
pub use concat::PathConcat;
pub use owned::{OwnedPath, OwnedSegment};
pub use simple::SimpleSegment;

/// Syntactic sugar for creating a pre-parsed path.
///
/// Example: `path!("foo", 4, "bar")` is the pre-parsed path of `foo[4].bar`
#[macro_export]
macro_rules! path {
    ($($segment:expr),*) => {{
           &[$($crate::lookup_v2::BorrowedSegment::from($segment),)*]
    }};
}

/// Syntactic sugar for creating a pre-parsed owned path.
///
/// This allocates and will be slower than using `path!`. Prefer that when possible.
/// The return value must be borrowed to get a value that implements `Path`.
///
/// Example: `owned_path!("foo", 4, "bar")` is the pre-parsed path of `foo[4].bar`
#[macro_export]
macro_rules! owned_path {
    ($($segment:expr),*) => {{
           $crate::lookup_v2::OwnedPath::from(vec![$($crate::lookup_v2::OwnedSegment::from($segment),)*])
    }};
}

/// Use if you want to pre-parse paths so it can be used multiple times.
/// The return value (when borrowed) implements `Path` so it can be used directly.
pub fn parse_path(path: &str) -> OwnedPath {
    let segments = JitPath::new(path)
        .segment_iter()
        .map(|segment| segment.into())
        .collect();
    OwnedPath { segments }
}

/// A path is simply the data describing how to look up a value.
/// This should only be implemented for types that are very cheap to clone, such as references.
pub trait Path<'a>: Clone {
    type Iter: Iterator<Item = BorrowedSegment<'a>> + Clone;

    /// Iterates over the raw "Borrowed" segments. This should be very fast (no memory allocations)
    fn segment_iter(&self) -> Self::Iter;

    /// Pre-processes the segments and then iterates over them. This returns segments that
    /// are easier to work with, but requires memory allocations. Only use in places where
    /// performance isn't critical.
    fn simple_segment_iter(&self) -> Result<SimpleSegmentIter<Self::Iter>, ()> {
        // check for invalid segments so the "SimpleSegment" enum doesn't need to contain an invalid variant
        if self
            .segment_iter()
            .any(|segment| segment == BorrowedSegment::Invalid)
        {
            return Err(());
        }
        Ok(SimpleSegmentIter::new(self.segment_iter()))
    }

    fn concat<T: Path<'a>>(&self, path: T) -> PathConcat<Self, T> {
        PathConcat {
            a: self.clone(),
            b: path,
        }
    }

    fn eq(&self, other: impl Path<'a>) -> bool {
        self.segment_iter().eq(other.segment_iter())
    }
}

impl<'a> Path<'a> for &'a str {
    type Iter = JitLookup<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitPath::new(self).segment_iter()
    }
}
