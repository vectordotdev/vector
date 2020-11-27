use crate::{Path, Value};

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
    fn insert(&mut self, path: &Path, value: Value) -> Result<(), String>;

    /// Get a value for a given path, or `None` if no value exists for the given
    /// path.
    ///
    /// See [`Object::insert`] for more details.
    fn get(&self, path: &Path) -> Result<Option<Value>, String>;

    /// Get the list of paths in the object.
    ///
    /// Paths are represented similar to what's documented in [`Object::insert`].
    fn paths(&self) -> Result<Vec<Path>, String>;

    /// Remove the given path from the object.
    ///
    /// If `compact` is true, after deletion, if an empty object or array is
    /// left behind, it should be removed as well.
    fn remove(&mut self, path: &Path, compact: bool) -> Result<(), String>;

    /// Return the type schema belonging to the object.
    ///
    /// This schema informs Remap on which paths are expected to exist in the
    /// object, and what value type(s) each path contains.
    ///
    /// FIXME: this can't live here, because we have to fetch this information
    /// at compile-time, not runtime...
    fn schema(&self) -> Option<Schema> {
        None
    }
}

// TODO
pub struct Schema;
