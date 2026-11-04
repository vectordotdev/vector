// No `use inventory` here — the whole point is that callers of the macro must
// not need a direct inventory dependency.
vector_lib::register_extra_span_field!("lib_integration_label");

/// Asserts that `register_extra_span_field!` registers the name as a `MetricLabel`
/// when called from a crate that has no direct `inventory` dependency.
///
/// If the `MetricLabel` arm were removed from the macro, this test would fail
/// because `lib_integration_label` would be absent from `LABELS` and
/// `VectorLabelFilter` would drop it before the metrics registry ever sees it.
#[test]
fn macro_registers_metric_label_without_caller_importing_inventory() {
    assert!(
        vector_lib::metrics::LABELS.contains("lib_integration_label"),
        "expected `lib_integration_label` to be in the MetricLabel allowlist",
    );
}
