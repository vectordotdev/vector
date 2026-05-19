//! How metric tags are exposed to and accepted from VRL or Lua.
//!
//! This enum lives in its own always-compiled module (rather than inside
//! `vrl_target`, which is gated on the `vrl` feature) so that the `lua`
//! feature can depend on the same type without being forced to also enable
//! `vrl`. The `vrl_target` and `lua` modules both re-export it.
//!
//! It mirrors `codecs::MetricTagValues`, but lives in `vector-core` so that
//! the crate dependency direction (`codecs -> vector-core`) is preserved.
//! Callers at the `codecs::MetricTagValues` boundary translate using the
//! `From<MetricTagValues>` impl on the codecs side.

/// How metric tags are exposed to and accepted from VRL or Lua.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum MetricTagMode {
    /// Tags are exposed as single strings (last value wins for multi-value
    /// tags); writes always produce single-value tags.
    #[default]
    Single,
    /// Tags are always exposed as arrays; writes always produce multi-value
    /// tags regardless of whether the assigned value is scalar or array.
    Full,
    /// Tags are exposed using their underlying shape: single-value tags as
    /// strings, multi-value tags as arrays. Writes: scalar values produce
    /// single-value tags; arrays of length >= 2 produce multi-value tags.
    ///
    /// A length-1 array is normalised to a single-value tag by the metric
    /// storage layer (`TagValueSet::Set` is never reduced below 2 elements),
    /// so an assignment like `.tags.region = ["us-east-1"]` round-trips as
    /// a scalar on the next read. Use `Full` to force array shape regardless
    /// of length.
    Auto,
}
