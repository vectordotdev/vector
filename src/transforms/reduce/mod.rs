use vector_lib::configurable::configurable_component;

#[cfg(any(feature = "transforms-reduce", feature = "transforms-impl-reduce"))]
pub mod config;

#[cfg(any(feature = "transforms-reduce", feature = "transforms-impl-reduce"))]
pub mod merge_strategy;

#[cfg(feature = "transforms-impl-reduce")]
pub mod transform;

/// Strategies for merging events.
#[configurable_component]
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Discard all but the first value found.
    Discard,

    /// Discard all but the last value found.
    ///
    /// Works as a way to coalesce by not retaining `null`.
    Retain,

    /// Sum all numeric values.
    Sum,

    /// Keep the maximum numeric value seen.
    Max,

    /// Keep the minimum numeric value seen.
    Min,

    /// Append each value to an array.
    Array,

    /// Concatenate each string value, delimited with a space.
    Concat,

    /// Concatenate each string value, delimited with a newline.
    ConcatNewline,

    /// Concatenate each string, without a delimiter.
    ConcatRaw,

    /// Keep the shortest array seen.
    ShortestArray,

    /// Keep the longest array seen.
    LongestArray,

    /// Create a flattened array of all unique values.
    FlatUnique,
}
