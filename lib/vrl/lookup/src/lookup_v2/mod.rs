//! This module contains all of the logic for paths.
//!
//! Paths can be thought of as similar to file paths (in an operating system) pointing
//! to specific files inside of a directory.
//! A `Value` is a data structure that can contain recursively nested fields. Paths
//! allow referring to a specific field inside of a `Value`.
//!
//! # Example
//! Below is a sample `Value`. Different fields can be accessed with paths.
//! ```json
//! {
//!   "foo": {
//!       "bar": 1
//!    },
//!    "baz": ["a", "b", "c"]
//! }
//! ```
//!
//! | path   | value it points to                    |
//! |--------|---------------------------------------|
//! | `.foo.bar` | `1`
//! | `.foo` | `{ "bar": 1 }`
//! | `.`    | `{ "foo" : { "bar" : 1 }, "baz" : ["a", "b", "c"] }`
//! | `.baz[0]` | `"a"`
//! | `.baz` | `["a", "b", "c"]`
//!
//!
//! # Traits
//! There are 2 main traits that define a path. Most functions that use a path for querying
//! will require one of these traits, rather than a concrete type.
//!
//! - [ValuePath] is a path that points to a field inside of a `Value`.
//! - [TargetPath] is a path that points to a field inside of a `target`. A `target` in VRL refers to
//! the external data being processed by a VRL script. A `target` has two main sections that can be
//! pointed to, `event` and `metadata`.  [TargetPath::prefix] identifies the section, and
//! [TargetPath::value_path] is a [ValuePath] pointing into that section.
//!
//! Note that for performance reasons, since [ValuePath] and [TargetPath] require [Clone], these
//! traits are only implemented on types that are cheap to clone (usually references). That means
//! when passing in a value (e.g. [OwnedValuePath]) into a function that requires `impl ValuePath`,
//! it will generally need to be passed in as a reference.
//!
//! # Owned Paths
//! [OwnedValuePath] and [OwnedTargetPath] are pre-parsed paths. That means that accessing fields
//! using an owned path is very fast. There is an upfront cost however, since owned paths are parsed
//! when they are created, and the segments are heap allocated. Owned paths should be preferred
//! if they can be created when performance isn't as much of a concern (e.g. startup time)
//! and they can be stored for re-use.
//! Owned paths tend to be easier to work with since you can directly access / manipulate the
//! segments that make up the path.
//!
//! If a path is being created and will only be used once, it may make sense to use other types.
//! For example here are two different ways to append a segment to a [OwnedValuePath]  before querying
//! a `Value`:
//! - Use [OwnedValuePath::with_field_appended] to create a new [OwnedValuePath] and use that. This
//! method is preferred if the new path will be used multiple times and the path adjustment can be
//! done in a non performance-critical part of the code (e.g. at startup).
//! - Use [ValuePath::concat] which con concatenate two [ValuePath]'s very efficiently without
//! allocating on the heap.
//!
//! To convert a string into an owned path, use either [parse_value_path] or [parse_target_path].
//!
//! # String Paths
//! [ValuePath] and [TargetPath] are implemented for [&str]. That means a raw / unparsed string can
//! be used as a path. This use is discouraged, and may be removed in the future. It mostly
//! exists for backwards compatibility in places where String paths are used instead of owned paths.
//! Using string paths is slightly slower than using an owned path. It's still very fast
//! but it is easy to introduce bugs since some compile-time type information is missing -
//! such as whether it is a target vs value path, or if the entire string is meant
//!  to be treated as a single segment vs being parsed as a path.
//!
//! # Macros
//! Several macros exist to make creating paths easier. These are used if the structure of the
//! path being created is already known. <strong>The macros do not parse paths</strong>. Use
//! [parse_value_path] or [parse_target_path] instead if the path needs to be parsed.
//!
//! You need to pass in each segment into the macro as separate arguments. A single argument is treated as
//! a single segment. This is true for all of the path macros.
//!
//! For example, [owned_value_path!][crate::owned_value_path] can be used to easily created owned paths.
//! - `owned_value_path!("foo.bar", "x")` will create a path with *two* segments. Equivalent to `."foo.bar".x`
//!

mod borrowed;
mod compat;
mod concat;
mod jit;
mod owned;

use self::jit::{JitValuePath, JitValuePathIter};
use snafu::Snafu;
use std::fmt::Debug;

pub use borrowed::BorrowedSegment;
pub use concat::PathConcat;
pub use owned::{OwnedSegment, OwnedTargetPath, OwnedValuePath};

#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum PathParseError {
    #[snafu(display("Invalid field path {:?}", path))]
    InvalidPathSyntax { path: String },
}

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

/// Used to pre-parse a path.
/// The return value (when borrowed) implements `Path` so it can be used directly.
/// This parses a value path, which is a path without a target prefix.
///
/// See `parse_target_path` if the path contains a target prefix.
pub fn parse_value_path(path: &str) -> Result<OwnedValuePath, PathParseError> {
    JitValuePath::new(path)
        .to_owned_value_path()
        .map_err(|_| PathParseError::InvalidPathSyntax {
            path: path.to_owned(),
        })
}

/// Used to pre-parse a path.
/// The return value (when borrowed) implements `Path` so it can be used directly.
/// This parses a target path, which is a path that contains a target prefix.
///
/// See `parse_value_path` if the path doesn't contain a prefix.
pub fn parse_target_path(path: &str) -> Result<OwnedTargetPath, PathParseError> {
    let prefix = TargetPath::prefix(&path);
    let value_path = parse_value_path(TargetPath::value_path(&path))?;

    Ok(OwnedTargetPath {
        prefix,
        path: value_path,
    })
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
        let (self_path, prefix_path) = if let (Ok(self_path), Ok(prefix_path)) =
            (self.to_owned_value_path(), prefix.to_owned_value_path())
        {
            (self_path, prefix_path)
        } else {
            return false;
        };

        let mut self_segments = self_path.segments.into_iter();
        for prefix_segment in prefix_path.segments.iter() {
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

    #[allow(clippy::result_unit_err)]
    fn to_owned_value_path(&self) -> Result<OwnedValuePath, ()> {
        let mut owned_path = OwnedValuePath::root();
        let mut coalesce = vec![];
        for segment in self.segment_iter() {
            match segment {
                BorrowedSegment::Invalid => return Err(()),
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
        Ok(owned_path)
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

impl<'a> TargetPath<'a> for &'a OwnedTargetPath {
    type ValuePath = &'a OwnedValuePath;

    fn prefix(&self) -> PathPrefix {
        self.prefix
    }

    fn value_path(&self) -> Self::ValuePath {
        &self.path
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

#[cfg(test)]
mod test {
    use crate::lookup_v2::parse_target_path;
    use crate::OwnedTargetPath;

    #[test]
    fn test_parse_target_path() {
        assert_eq!(
            parse_target_path("i"),
            Ok(OwnedTargetPath::event(owned_value_path!("i")))
        );
    }
}
