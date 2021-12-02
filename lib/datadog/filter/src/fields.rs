use datadog_search_syntax::Field;

/// A `Fielder` is type that can build and return an `IntoIterator` of Datadog Search
/// Syntax `Field`s. These are intended to be passed along to `Filter` methods as pre-parsed
/// field types which can be used to determine which logic is necessary to match against.
pub trait Fielder {
    type IntoIter: IntoIterator<Item = Field>;

    /// Builds fields, and returns an iterator. Takes a mutable ref to self to enable caching
    /// scenarios where further lookups or other expensive operations can be performed at boot-time.
    fn build_fields(&mut self, attr: impl AsRef<str>) -> Self::IntoIter;
}
