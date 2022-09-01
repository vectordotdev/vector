mod borrowed;
mod compat;
mod concat;
mod jit;
mod owned;

use self::jit::{JitLookup, JitPath};
use std::fmt::{Debug, Display, Formatter};

pub use borrowed::BorrowedSegment;
pub use concat::PathConcat;
pub use owned::{OwnedPath, OwnedSegment};

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
    JitPath::new(path).to_owned_path()
}

/// A path is simply the data describing how to look up a value.
/// This should only be implemented for types that are very cheap to clone, such as references.
pub trait Path<'a>: Clone {
    type Iter: Iterator<Item = BorrowedSegment<'a>> + Clone;

    /// Iterates over the raw "Borrowed" segments.
    fn segment_iter(&self) -> Self::Iter;

    fn concat<T: Path<'a>>(&self, path: T) -> PathConcat<Self, T> {
        PathConcat {
            a: self.clone(),
            b: path,
        }
    }

    fn eq(&self, other: impl Path<'a>) -> bool {
        self.segment_iter().eq(other.segment_iter())
    }

    fn can_start_with(&self, prefix: impl Path<'a>) -> bool {
        let mut self_segments = self.to_owned_path().segments.into_iter();
        for prefix_segment in prefix.to_owned_path().segments.iter() {
            match self_segments.next() {
                None => return false,
                Some(self_segment) => {
                    if !self_segment.can_start_with(prefix_segment) {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn to_owned_path(&self) -> OwnedPath {
        let mut owned_path = OwnedPath::root();
        let mut coalesce = vec![];
        for segment in self.segment_iter() {
            match segment {
                BorrowedSegment::Invalid => return OwnedPath::invalid(),
                BorrowedSegment::Index(i) => owned_path.push(OwnedSegment::Index(i)),
                BorrowedSegment::Field(field) => {
                    owned_path.push(OwnedSegment::Field(field.to_string()))
                }
                BorrowedSegment::CoalesceField(field) => {
                    coalesce.push(field.to_string());
                }
                BorrowedSegment::CoalesceEnd(field) => {
                    coalesce.push(field.to_string());
                    owned_path.push(OwnedSegment::Coalesce(std::mem::take(&mut coalesce)));
                }
            }
        }
        owned_path
    }
}

impl<'a> Path<'a> for &'a str {
    type Iter = JitLookup<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitPath::new(self).segment_iter()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PathPrefix {
    Event,
    Metadata,
}

#[derive(Hash, Eq, PartialEq, Clone, PartialOrd, Ord)]
pub struct TargetPath {
    pub prefix: PathPrefix,
    pub path: OwnedPath,
}

impl TargetPath {
    pub fn event_root() -> Self {
        Self::root(PathPrefix::Event)
    }

    pub fn metadata_root() -> Self {
        Self::root(PathPrefix::Metadata)
    }

    pub fn root(prefix: PathPrefix) -> Self {
        Self {
            prefix,
            path: OwnedPath::root(),
        }
    }

    pub fn event(path: OwnedPath) -> Self {
        Self {
            prefix: PathPrefix::Event,
            path,
        }
    }

    pub fn metadata(path: OwnedPath) -> Self {
        Self {
            prefix: PathPrefix::Metadata,
            path,
        }
    }

    pub fn can_start_with(&self, prefix: &Self) -> bool {
        if self.prefix != prefix.prefix {
            return false;
        }
        (&self.path).can_start_with(&prefix.path)
    }
}

impl Display for TargetPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.prefix {
            PathPrefix::Event => write!(f, ".{}", self.path),
            PathPrefix::Metadata => write!(f, "%{}", self.path),
        }
    }
}

impl Debug for TargetPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}
