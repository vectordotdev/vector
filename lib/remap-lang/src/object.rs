use std::fmt::Display;

use crate::{Path, Value};

#[derive(Clone, Debug, PartialEq)]
/// A wrapper around a vector of strings.
/// When an invalid path is sent to an Object, a list of valid paths can be returned
/// in the error message and displayed to the user.
pub struct ValidPaths(pub Vec<&'static str>);

impl Display for ValidPaths {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join(","))
    }
}


#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("cannot set root path")]
    SetRoot,

    #[error("{0}")]
    InvalidType(String),

    #[error("unable to create path from alternative string: {0}, expected one of {1}")]
    InvalidPath(String, ValidPaths),

    /// Other error messages specific to the implementation of object.
    #[error("{0}")]
    Other(String),
}

/// Any object you want to map through the remap language has to implement this
/// trait.
pub trait Object: std::fmt::Debug {
    /// Insert a given [`Value`] in the provided [`Object`].
    ///
    /// The `path` parameter determines _where_ in the given object the value
    /// should be inserted.
    ///
    /// A path contains dot-delimited segments, and can contain a combination
    /// of:
    ///
    /// * regular path segments:
    ///
    ///   ```txt
    ///   .foo.bar.baz
    ///   ```
    ///
    /// * quoted path segments:
    ///
    ///   ```txt
    ///   .foo."bar.baz"
    ///   ```
    ///
    /// * coalesced path segments:
    ///
    ///   ```txt
    ///   .foo.(bar | "bar.baz").qux
    ///   ```
    ///
    /// * path indices:
    ///
    ///   ```txt
    ///   .foo[2]
    ///   ```
    ///
    /// When inserting into a coalesced path, the implementor is encouraged to
    /// insert into the right-most segment if none exists, but can return an
    /// error if needed.
    fn insert(&mut self, path: &Path, value: Value) -> Result<(), Error>;

    /// Get a value for a given path, or `None` if no value exists for the given
    /// path.
    ///
    /// See [`Object::insert`] for more details.
    fn get(&self, path: &Path) -> Result<Option<Value>, Error>;

    /// Get the list of paths in the object.
    ///
    /// Paths are represented similar to what's documented in [`Object::insert`].
    fn paths(&self) -> Result<Vec<Path>, Error>;

    /// Remove the given path from the object.
    ///
    /// If `compact` is true, after deletion, if an empty object or array is
    /// left behind, it should be removed as well.
    fn remove(&mut self, path: &Path, compact: bool) -> Result<(), Error>;
}
