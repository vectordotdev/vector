use crate::{Path, Value};

/// Any target object you want to remap using VRL has to implement this trait.
pub trait Target: std::fmt::Debug {
    /// Insert a given [`Value`] in the provided [`Target`].
    ///
    /// The `path` parameter determines _where_ in the given target the value
    /// should be inserted.
    ///
    /// A path consists of "path segments". Each segment can be one of:
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
    ///   .foo[2][-1]
    ///   ```
    ///
    /// When inserting into a coalesced path, the implementor is encouraged to
    /// insert into the right-most segment if none exists, but can return an
    /// error if needed.
    fn insert(&mut self, path: &Path, value: Value) -> Result<(), String>;

    /// Get a value for a given path, or `None` if no value is found.
    ///
    /// See [`Object::insert`] for more details.
    fn get(&self, path: &Path) -> Result<Option<Value>, String>;

    /// Remove the given path from the object.
    ///
    /// Returns the removed object, if any.
    ///
    /// If `compact` is true, after deletion, if an empty object or array is
    /// left behind, it should be removed as well, cascading up to the root.
    fn remove(&mut self, path: &Path, compact: bool) -> Result<Option<Value>, String>;
}
