use datadog_search_syntax::{normalize_fields, Field};

/// A `Resolver` is type that can build and return an `IntoIterator` of Datadog Search
/// Syntax `Field`s. These are intended to be passed along to `Filter` methods as pre-parsed
/// field types which can be used to determine which logic is necessary to match against.
pub trait Resolver {
    /// Builds fields, and returns an iterator. Takes a immutable ref to self to allow for
    /// recursion when building filters. A type that implements `Resolver` + `Filter` and needs
    /// to update an internal cache when building fields should use interior mutability.
    fn build_fields(&self, attr: &str) -> Vec<Field> {
        normalize_fields(attr).into_iter().collect()
    }
}
