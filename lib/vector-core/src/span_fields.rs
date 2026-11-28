/// Span field name that should be captured onto log events emitted by the `internal_logs`
/// source. Vector's `SpanFields` visitor only captures fields `component_*` by default;
/// downstream crates can extend that set through this type.
///
/// Use [`register_extra_span_field!`](crate::register_extra_span_field) to register one.
#[derive(Debug)]
pub struct SpanField(pub &'static str); // name of the span field

inventory::collect!(SpanField); // collect the span field names

/// Register a tracing-span field name that downstream crates want preserved on Vector's
/// internal observability output.
///
/// A single registration covers both output channels:
///
/// * On metrics, the field is added to the allowlist consulted by
///   [`VectorLabelFilter`](crate::metrics) (alongside Vector's built-in `component_id`,
///   `component_type`, `component_kind`, `buffer_type`), so `metrics-tracing-context` no
///   longer drops it before the metrics registry sees it.
/// * On logs/traces emitted via `internal_logs`, the field is added to the allowlist
///   consulted by `SpanFields` (alongside the existing `component_*` prefix gate), so it is
///   captured onto the log event under `vector.<field>`.
///
/// Example: an embedder that owns a "deployment-version" concept of its own can write
/// `register_extra_span_field!("deployment_version");` once at module scope and any internal
/// metric or log emitted from inside a span carrying that field will inherit it.
///
/// Registrations are collected at link time via the `inventory` crate, so both read paths
/// are lock-free. The expansion goes through this crate's re-exports of `inventory`,
/// [`MetricLabel`](crate::metrics::MetricLabel), and [`SpanField`], so callers do not need
/// a direct `inventory` dependency.
#[macro_export]
macro_rules! register_extra_span_field {
    ($key:expr) => {
        $crate::__inventory::submit! {
            $crate::metrics::MetricLabel($key)
        }
        $crate::__inventory::submit! {
            $crate::SpanField($key)
        }
    };
}
