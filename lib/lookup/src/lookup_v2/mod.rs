mod borrowed;
mod compat;
mod concat;
mod jit;
mod owned;

use self::jit::{JitValuePath, JitValuePathIter};
use std::fmt::{Debug, Display, Formatter};

pub use borrowed::BorrowedSegment;
pub use concat::PathConcat;
pub use owned::{OwnedSegment, OwnedValuePath};

/// Syntactic sugar for creating a pre-parsed path.
///
/// Example: `path!("foo", 4, "bar")` is the pre-parsed path of `foo[4].bar`
#[macro_export]
macro_rules! path {
    ($($segment:expr),*) => {{
           &[$($crate::lookup_v2::BorrowedSegment::from($segment),)*]
    }};
}

/// Syntactic sugar for creating a pre-parsed path.
/// This path points at an event (as opposed to metadata).
#[macro_export]
macro_rules! event_path {
    ($($segment:expr),*) => {{
           ($crate::lookup_v2::PathPrefix::Event, &[$($crate::lookup_v2::BorrowedSegment::from($segment),)*])
    }};
}

/// Syntactic sugar for creating a pre-parsed path.
/// This path points at metadata (as opposed to the event).
#[macro_export]
macro_rules! metadata_path {
    ($($segment:expr),*) => {{
           ($crate::lookup_v2::PathPrefix::Metadata, &[$($crate::lookup_v2::BorrowedSegment::from($segment),)*])
    }};
}

/// Syntactic sugar for creating a pre-parsed owned path.
///
/// This allocates and will be slower than using `path!`. Prefer that when possible.
/// The return value must be borrowed to get a value that implements `Path`.
///
/// Example: `owned_value_path!("foo", 4, "bar")` is the pre-parsed path of `foo[4].bar`
#[macro_export]
macro_rules! owned_value_path {
    ($($segment:expr),*) => {{
           $crate::lookup_v2::OwnedValuePath::from(vec![$($crate::lookup_v2::OwnedSegment::from($segment),)*])
    }};
}

/// Use if you want to pre-parse paths so it can be used multiple times.
/// The return value (when borrowed) implements `Path` so it can be used directly.
pub fn parse_value_path(path: &str) -> OwnedValuePath {
    JitValuePath::new(path).to_owned_value_path()
}

pub trait TargetPath<'a>: Clone {
    type ValuePath: ValuePath<'a>;

    fn prefix(&self) -> PathPrefix;
    fn value_path(&self) -> Self::ValuePath;
}

/// A path is simply the data describing how to look up a field from a value.
/// This should only be implemented for types that are very cheap to clone, such as references.
pub trait ValuePath<'a>: Clone {
    type Iter: Iterator<Item = BorrowedSegment<'a>> + Clone;

    /// Iterates over the raw "Borrowed" segments.
    fn segment_iter(&self) -> Self::Iter;

    fn concat<T: ValuePath<'a>>(&self, path: T) -> PathConcat<Self, T> {
        PathConcat {
            a: self.clone(),
            b: path,
        }
    }

    fn eq(&self, other: impl ValuePath<'a>) -> bool {
        self.segment_iter().eq(other.segment_iter())
    }

    fn can_start_with(&self, prefix: impl ValuePath<'a>) -> bool {
        let mut self_segments = self.to_owned_value_path().segments.into_iter();
        for prefix_segment in prefix.to_owned_value_path().segments.iter() {
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

    fn to_owned_value_path(&self) -> OwnedValuePath {
        let mut owned_path = OwnedValuePath::root();
        let mut coalesce = vec![];
        for segment in self.segment_iter() {
            match segment {
                BorrowedSegment::Invalid => return OwnedValuePath::invalid(),
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

impl<'a> ValuePath<'a> for &'a str {
    type Iter = JitValuePathIter<'a>;

    fn segment_iter(&self) -> Self::Iter {
        JitValuePath::new(self).segment_iter()
    }
}

impl<'a> TargetPath<'a> for &'a str {
    type ValuePath = &'a str;

    fn prefix(&self) -> PathPrefix {
        get_target_prefix(self).0
    }

    fn value_path(&self) -> Self::ValuePath {
        get_target_prefix(self).1
    }
}

impl<'a, T: ValuePath<'a>> TargetPath<'a> for (PathPrefix, T) {
    type ValuePath = T;

    fn prefix(&self) -> PathPrefix {
        self.0
    }

    fn value_path(&self) -> Self::ValuePath {
        self.1.clone()
    }
}

/// Determines the prefix of a "TargetPath", and also returns the remaining
/// "ValuePath" portion of the string.
fn get_target_prefix(path: &str) -> (PathPrefix, &str) {
    match path.chars().next() {
        Some('.') => {
            // For backwards compatibility, the "ValuePath" parser still allows an optional
            // starting ".". To prevent ".." from being a valid path, it is _not_ removed
            // here. This should be changed once "ValuePath" no longer allows a leading ".".
            (PathPrefix::Event, path)
        }
        Some('%') => (PathPrefix::Metadata, &path[1..]),
        _ => {
            // This shouldn't be allowed in the future, but is currently
            // used for backwards compatibility.
            (PathPrefix::Event, path)
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PathPrefix {
    Event,
    Metadata,
}

#[derive(Hash, Eq, PartialEq, Clone, PartialOrd, Ord)]
pub struct OwnedTargetPath {
    pub prefix: PathPrefix,
    pub path: OwnedValuePath,
}

impl OwnedTargetPath {
    pub fn event_root() -> Self {
        Self::root(PathPrefix::Event)
    }
    pub fn metadata_root() -> Self {
        Self::root(PathPrefix::Metadata)
    }

    pub fn root(prefix: PathPrefix) -> Self {
        Self {
            prefix,
            path: OwnedValuePath::root(),
        }
    }

    pub fn event(path: OwnedValuePath) -> Self {
        Self {
            prefix: PathPrefix::Event,
            path,
        }
    }

    pub fn metadata(path: OwnedValuePath) -> Self {
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

impl Display for OwnedTargetPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.prefix {
            PathPrefix::Event => write!(f, ".{}", self.path),
            PathPrefix::Metadata => write!(f, "%{}", self.path),
        }
    }
}

impl Debug for OwnedTargetPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}
